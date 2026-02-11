use std::collections::HashSet;
use std::io::{BufRead, BufReader};

use attentive_core::AttentionState;
use attentive_telemetry::Paths;

pub fn run(session: Option<&str>) -> anyhow::Result<()> {
    let paths = Paths::new()?;
    let state_path = paths.attn_state_path()?;

    let state: Option<AttentionState> = if state_path.exists() {
        std::fs::read_to_string(&state_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
    } else {
        None
    };

    let (hot, warm, cold, injected_files) = match &state {
        Some(s) => {
            let mut h = 0usize;
            let mut w = 0usize;
            let mut c = 0usize;
            let mut injected = HashSet::new();
            for (path, &score) in &s.scores {
                if score >= 0.8 {
                    h += 1;
                    injected.insert(path.clone());
                } else if score >= 0.25 {
                    w += 1;
                    injected.insert(path.clone());
                } else {
                    c += 1;
                }
            }
            (h, w, c, injected)
        }
        None => (0, 0, 0, HashSet::new()),
    };

    let hit_rate = session
        .and_then(|sid| {
            let project_dir = paths.project_dir().ok()?;
            let transcript = project_dir.join(format!("{sid}.jsonl"));
            compute_hit_rate_from_transcript(&transcript, &injected_files)
        })
        .unwrap_or(-1);

    let output = serde_json::json!({
        "hot": hot,
        "warm": warm,
        "cold": cold,
        "hit_rate": hit_rate,
    });
    println!("{output}");
    Ok(())
}

fn compute_hit_rate_from_transcript(
    transcript: &std::path::Path,
    injected_files: &HashSet<String>,
) -> Option<i32> {
    if injected_files.is_empty() {
        return None;
    }
    let file = std::fs::File::open(transcript).ok()?;
    let reader = BufReader::new(file);
    let mut used_files = HashSet::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let turn: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if turn.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let content = turn.pointer("/message/content").and_then(|c| c.as_array());
        let Some(content) = content else { continue };
        for item in content {
            if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                continue;
            }
            let Some(input) = item.get("input") else {
                continue;
            };
            let target = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .or_else(|| input.get("notebook_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if target.starts_with('/') {
                used_files.insert(target.to_string());
            }
        }
    }

    let overlap = injected_files.intersection(&used_files).count();
    Some((overlap as f64 / injected_files.len() as f64 * 100.0) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_rate_empty() {
        let injected = HashSet::new();
        let result =
            compute_hit_rate_from_transcript(std::path::Path::new("/nonexistent"), &injected);
        assert_eq!(result, None);
    }

    #[test]
    fn test_hit_rate_from_transcript() {
        let temp = tempfile::TempDir::new().unwrap();
        let transcript = temp.path().join("test.jsonl");
        let content = serde_json::json!({
            "type": "assistant",
            "message": {
                "content": [
                    {"type": "tool_use", "name": "Read", "input": {"file_path": "/src/a.rs"}},
                    {"type": "tool_use", "name": "Edit", "input": {"file_path": "/src/b.rs"}},
                ]
            }
        });
        std::fs::write(&transcript, format!("{}\n", content)).unwrap();

        let injected: HashSet<String> = ["/src/a.rs", "/src/b.rs", "/src/c.rs"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let rate = compute_hit_rate_from_transcript(&transcript, &injected);
        assert_eq!(rate, Some(66)); // 2/3
    }
}
