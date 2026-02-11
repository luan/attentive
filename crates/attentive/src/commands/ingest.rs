use attentive_learn::Learner;
use attentive_telemetry::Paths;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn extract_files_from_session_turn(turn: &serde_json::Value) -> Vec<String> {
    let mut files = HashSet::new();
    if let Some(content) = turn.pointer("/message/content").and_then(|c| c.as_array()) {
        for item in content {
            if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                continue;
            }
            let Some(input) = item.get("input") else {
                continue;
            };
            // Read, Edit, Write — file_path
            if let Some(p) = input.get("file_path").and_then(|v| v.as_str()) {
                files.insert(p.to_string());
            }
            // Grep, Glob — path
            if let Some(p) = input.get("path").and_then(|v| v.as_str()) {
                files.insert(p.to_string());
            }
            // Bash — extract file paths from command
            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                for token in cmd.split_whitespace() {
                    if token.contains('/') && !token.starts_with('-') && !token.contains("://") {
                        files.insert(token.to_string());
                    }
                }
            }
            // NotebookEdit — notebook_path
            if let Some(p) = input.get("notebook_path").and_then(|v| v.as_str()) {
                files.insert(p.to_string());
            }
        }
    }
    files.into_iter().collect()
}

fn extract_prompt_from_turn(turn: &serde_json::Value) -> String {
    let content = match turn.pointer("/message/content") {
        Some(c) => c,
        None => return String::new(),
    };
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) == Some("text")
                && let Some(text) = item.get("text").and_then(|t| t.as_str())
            {
                return text.to_string();
            }
        }
    }
    String::new()
}

type PromptFilePairs = Vec<(String, Vec<String>)>;

fn parse_session_jsonl(path: &Path) -> anyhow::Result<(PromptFilePairs, usize)> {
    let content = std::fs::read_to_string(path)?;
    let mut pairs = Vec::new();
    let mut current_prompt = String::new();
    let mut total = 0;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        total += 1;
        let turn = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let turn_type = turn.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match turn_type {
            "human" | "user" => {
                current_prompt = extract_prompt_from_turn(&turn);
            }
            "assistant" => {
                let files = extract_files_from_session_turn(&turn);
                if !current_prompt.is_empty() && !files.is_empty() {
                    pairs.push((current_prompt.clone(), files));
                }
            }
            _ => {}
        }
    }

    Ok((pairs, total))
}

fn discover_session_files(project_dir: &Path) -> Vec<PathBuf> {
    if !project_dir.exists() {
        return Vec::new();
    }

    let mut files = Vec::new();
    let dir_entries = match std::fs::read_dir(project_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    for file_entry in dir_entries.flatten() {
        let path = file_entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl") && path.is_file() {
            files.push(path);
        }
    }

    files
}

fn load_existing_learner(path: &Path) -> Learner {
    if !path.exists() {
        return Learner::new();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn run(file: Option<&str>) -> anyhow::Result<()> {
    let paths = Paths::new()?;
    let project_dir = paths.project_dir()?;
    let learned_state_path = paths.learned_state_path()?;
    std::fs::create_dir_all(&project_dir)?;

    let session_files: Vec<PathBuf> = if let Some(f) = file {
        vec![PathBuf::from(f)]
    } else {
        let files = discover_session_files(&project_dir);
        if files.is_empty() {
            println!("No session files found in {}", project_dir.display());
            return Ok(());
        }
        println!("Discovered {} session files", files.len());
        files
    };

    let mut learner = load_existing_learner(&learned_state_path);
    let initial_maturity = learner.maturity();

    let mut total_pairs = 0;
    let mut total_files_processed = 0;
    let mut per_session_info: Vec<(String, usize, usize)> = Vec::new();
    let mut last_session_files: Vec<String> = Vec::new();

    for path in &session_files {
        let (pairs, total_turns) = match parse_session_jsonl(path) {
            Ok(result) => result,
            Err(_) => continue,
        };
        if pairs.is_empty() {
            continue;
        }

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        per_session_info.push((filename, pairs.len(), total_turns));

        // Collect unique files from this session for warm-start
        let mut session_files_set = std::collections::HashSet::new();
        for (_prompt, files) in &pairs {
            for f in files {
                session_files_set.insert(f.clone());
            }
        }
        last_session_files = session_files_set.into_iter().collect();

        total_files_processed += 1;
        total_pairs += pairs.len();
        for (prompt, files) in &pairs {
            learner.observe_turn(prompt, files);
        }
    }

    if total_pairs == 0 {
        println!("No prompt-file pairs found");
        return Ok(());
    }

    learner.save_session(&last_session_files);
    let json = serde_json::to_string_pretty(&learner)?;
    attentive_telemetry::atomic_write(&learned_state_path, json.as_bytes())?;

    // Print per-session details
    for (filename, pairs, turns) in &per_session_info {
        println!("  {}: {} pairs from {} turns", filename, pairs, turns);
    }
    println!();

    println!(
        "Ingested {} pairs from {} sessions",
        total_pairs, total_files_processed
    );

    // Print top files learned
    let top_files = learner.top_files_by_frequency(10);
    if !top_files.is_empty() {
        println!("Top files learned:");
        for (file, count) in &top_files {
            println!("  {}  (seen {} turns)", file, count);
        }
    }

    // Print association count
    let associations = learner.total_associations();
    println!("Associations: {} word→file mappings", associations);

    println!(
        "Maturity: {:?} -> {:?}",
        initial_maturity,
        learner.maturity()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_files_from_session_turn() {
        let turn = serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "tool_use", "name": "Read", "input": {"file_path": "/src/router.rs"}},
                    {"type": "tool_use", "name": "Edit", "input": {"file_path": "/src/config.rs"}},
                    {"type": "tool_use", "name": "Bash", "input": {"command": "cargo test"}},
                ]
            }
        });
        let files = extract_files_from_session_turn(&turn);
        assert!(files.contains(&"/src/router.rs".to_string()));
        assert!(files.contains(&"/src/config.rs".to_string()));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_extract_prompt_from_turn() {
        let turn = serde_json::json!({
            "type": "human",
            "message": {
                "content": [{"type": "text", "text": "fix the router bug"}]
            }
        });
        assert_eq!(extract_prompt_from_turn(&turn), "fix the router bug");
    }

    #[test]
    fn test_parse_session_jsonl() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("session.jsonl");
        let lines = [
            serde_json::json!({"type": "human", "message": {"content": [{"type": "text", "text": "fix router"}]}}),
            serde_json::json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "Read", "input": {"file_path": "router.rs"}}]}}),
        ];
        let content: String = lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, content).unwrap();

        let (pairs, total) = parse_session_jsonl(&path).unwrap();
        assert_eq!(total, 2);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "fix router");
        assert!(pairs[0].1.contains(&"router.rs".to_string()));
    }

    #[test]
    fn test_parse_session_jsonl_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("empty.jsonl");
        std::fs::write(&path, "").unwrap();

        let (pairs, total) = parse_session_jsonl(&path).unwrap();
        assert_eq!(total, 0);
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_discover_session_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let project_dir = temp.path().join("my-project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(project_dir.join("abc.jsonl"), "{}").unwrap();
        std::fs::write(project_dir.join("not-jsonl.txt"), "{}").unwrap();

        let files = discover_session_files(&project_dir);
        assert_eq!(files.len(), 1);
        assert!(files[0].to_str().unwrap().ends_with(".jsonl"));
    }

    #[test]
    fn test_load_existing_learner_extends() {
        let temp = tempfile::TempDir::new().unwrap();
        let state_path = temp.path().join("learned_state.json");

        let mut learner = Learner::new();
        for _ in 0..10 {
            learner.observe_turn("router config", &["router.rs".to_string()]);
        }
        let json = serde_json::to_string(&learner).unwrap();
        std::fs::write(&state_path, &json).unwrap();

        let loaded = load_existing_learner(&state_path);
        assert_eq!(format!("{:?}", loaded.maturity()), "Observing");

        // Can be serialized back
        let roundtrip = serde_json::to_string(&loaded).unwrap();
        assert!(!roundtrip.is_empty());
    }

    #[test]
    fn test_load_existing_learner_invalid_json_returns_new() {
        let temp = tempfile::TempDir::new().unwrap();
        let state_path = temp.path().join("learned_state.json");
        std::fs::write(&state_path, r#"{"not": "a learner"}"#).unwrap();

        let loaded = load_existing_learner(&state_path);
        assert_eq!(format!("{:?}", loaded.maturity()), "Observing");
    }
}
