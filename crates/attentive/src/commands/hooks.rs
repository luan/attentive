use attentive_core::{AttentionState, Config, Router};
use attentive_plugins::PluginRegistry;
use attentive_telemetry::Paths;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::path::Path;

fn load_config(home_claude: &Path) -> Config {
    let config_path = home_claude.join("attentive.json");
    if !config_path.exists() {
        return Config::new();
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return Config::new(),
    };

    #[derive(Deserialize)]
    struct ConfigFile {
        #[serde(default)]
        co_activation: std::collections::HashMap<String, Vec<String>>,
        #[serde(default)]
        pinned_files: Vec<String>,
        #[serde(default)]
        demoted_files: Vec<String>,
    }

    match serde_json::from_str::<ConfigFile>(&content) {
        Ok(cf) => {
            let mut config = Config::new();
            config.co_activation = cf.co_activation;
            config.pinned_files = cf.pinned_files;
            config.demoted_files = cf.demoted_files;
            config
        }
        Err(_) => Config::new(),
    }
}

fn load_learner(state_path: &Path) -> Option<attentive_learn::Learner> {
    if !state_path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(state_path).ok()?;
    serde_json::from_str(&content).ok()
}

const MAX_TOTAL_CHARS: usize = 20000;

fn read_file_content(path: &str, max_chars: usize) -> String {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            if content.len() > max_chars {
                format!(
                    "{}...\n[truncated at {} chars]",
                    &content[..max_chars],
                    max_chars
                )
            } else {
                content
            }
        }
        Err(_) => format!("[error reading {}]", path),
    }
}

fn extract_toc(content: &str) -> String {
    let mut toc_lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let is_heading = trimmed.starts_with('#');
        let is_signature = trimmed.starts_with("pub fn ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("def ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("pub struct ")
            || trimmed.starts_with("pub enum ")
            || trimmed.starts_with("pub trait ");

        if is_heading || is_signature {
            toc_lines.push(trimmed.to_string());
        }
    }
    toc_lines.join("\n")
}

fn build_tiered_context(
    hot_files: &[String],
    warm_files: &[String],
    max_total_chars: usize,
) -> String {
    let mut parts = Vec::new();
    let mut chars_used = 0;
    let per_hot_budget = if !hot_files.is_empty() {
        (max_total_chars * 70 / 100) / hot_files.len()
    } else {
        0
    };

    for path in hot_files {
        if chars_used >= max_total_chars {
            break;
        }
        let content = read_file_content(path, per_hot_budget);
        let section = format!("[HOT] {}\n{}", path, content);
        chars_used += section.len();
        parts.push(section);
    }

    for path in warm_files {
        if chars_used >= max_total_chars {
            break;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => extract_toc(&c),
            Err(_) => format!("[error reading {}]", path),
        };
        let section = format!("[WARM] {} (TOC)\n{}", path, content);
        chars_used += section.len();
        parts.push(section);
    }

    parts.join("\n\n")
}

fn detect_project_switch(session_state_path: &Path, current_project: &str) -> bool {
    #[derive(Serialize, Deserialize, Default)]
    struct SessionState {
        #[serde(default)]
        current_project: String,
    }

    let mut state = if session_state_path.exists() {
        std::fs::read_to_string(session_state_path)
            .ok()
            .and_then(|c| serde_json::from_str::<SessionState>(&c).ok())
            .unwrap_or_default()
    } else {
        SessionState::default()
    };

    let switched = !state.current_project.is_empty() && state.current_project != current_project;

    state.current_project = current_project.to_string();
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = attentive_telemetry::atomic_write(session_state_path, json.as_bytes());
    }

    switched
}

fn build_dashboard(
    turns: &[attentive_telemetry::TurnRecord],
    _learner: Option<&attentive_learn::Learner>,
) -> String {
    if turns.is_empty() {
        return String::new();
    }

    let mut lines = vec!["## attentive".to_string()];

    // Waste metrics
    let waste_ratios: Vec<f64> = turns
        .iter()
        .filter(|t| t.waste_ratio >= 0.0)
        .map(|t| t.waste_ratio)
        .collect();
    if !waste_ratios.is_empty() {
        let avg_waste = waste_ratios.iter().sum::<f64>() / waste_ratios.len() as f64;
        let notif_count = turns.iter().filter(|t| t.was_notification).count();
        let notif_pct = notif_count as f64 / turns.len() as f64 * 100.0;
        lines.push(format!(
            "Waste: {:.0}% | Notifs filtered: {}/{} ({:.0}%)",
            avg_waste * 100.0,
            notif_count,
            turns.len(),
            notif_pct
        ));
    }

    // Top wasted files
    let mut file_injected: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    let mut file_used: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for t in turns {
        for f in &t.files_injected {
            *file_injected.entry(f.as_str()).or_default() += 1;
        }
        for f in &t.files_used {
            *file_used.entry(f.as_str()).or_default() += 1;
        }
    }
    let mut waste_sorted: Vec<_> = file_injected
        .iter()
        .map(|(f, &inj)| (*f, inj, *file_used.get(f).unwrap_or(&0)))
        .collect();
    waste_sorted.sort_by(|a, b| (b.1 as i64 - b.2 as i64).cmp(&(a.1 as i64 - a.2 as i64)));
    if !waste_sorted.is_empty() {
        let top3: Vec<String> = waste_sorted
            .iter()
            .take(3)
            .map(|(f, inj, used)| {
                let name = Path::new(f)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(f);
                format!("{}({}i/{}u)", name, inj, used)
            })
            .collect();
        lines.push(format!("Top waste: {}", top3.join(", ")));
    }

    lines.join("\n")
}

#[derive(Debug, Deserialize)]
struct PromptInput {
    prompt: String,
}

#[derive(Debug, Serialize)]
struct PromptOutput {
    context: String,
    metadata: serde_json::Value,
}

pub fn hook_user_prompt_submit() -> anyhow::Result<()> {
    // 1. Read JSON from stdin
    let mut input_str = String::new();
    io::stdin().read_to_string(&mut input_str)?;

    let input: PromptInput = serde_json::from_str(&input_str)?;

    // 2. Load or create attention state
    let paths = Paths::new()?;
    let project_dir = paths.project_dir()?;
    std::fs::create_dir_all(&project_dir)?;

    let state_path = paths.attn_state_path()?;
    let mut state = if state_path.exists() {
        let content = std::fs::read_to_string(&state_path)?;
        serde_json::from_str(&content)?
    } else {
        AttentionState::new()
    };

    // 3. Create router with loaded config
    let config = load_config(&paths.home_claude);
    let router = Router::new(config);

    // 4. Initialize plugins
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(attentive_plugins::BurnRatePlugin::new()));
    registry.register(Box::new(attentive_plugins::LoopBreakerPlugin::new()));
    registry.register(Box::new(attentive_plugins::VerifyFirstPlugin::new()));

    // 5. Run plugin pre-hooks
    let session_state = std::collections::HashMap::new();
    let (prompt, should_continue) = registry.on_prompt_pre(input.prompt, &session_state);

    if !should_continue {
        return Ok(());
    }

    // 6. Seed learned files, run router, then restore seeds that decay killed
    let learned_state_path = paths.learned_state_path()?;
    let learner = load_learner(&learned_state_path);
    let mut seed_scores: Vec<(String, f64)> = Vec::new();
    if let Some(l) = &learner {
        for file in l.get_warmup() {
            if !state.scores.contains_key(&file) {
                state.scores.insert(file.clone(), 0.8);
                seed_scores.push((file, 0.8));
            }
        }
        for (file, _freq) in l.top_files_by_frequency(50) {
            if !state.scores.contains_key(&file) {
                state.scores.insert(file.clone(), 0.5);
                seed_scores.push((file, 0.5));
            }
        }
    }

    // 7. Run router (decay + learner boost)
    let _activated = router.update_attention(&mut state, &prompt, learner.as_ref());

    // Restore seed floors — decay shouldn't penalize files with no prior state
    for (file, floor) in &seed_scores {
        if let Some(score) = state.scores.get_mut(file) {
            *score = score.max(*floor);
        }
    }

    let (hot_files, warm_files, _cold_files) = router.build_context_output(&state);

    // 7. Build context string (HOT: full content, WARM: TOC, COLD: evicted)
    let context_output = build_tiered_context(&hot_files, &warm_files, MAX_TOTAL_CHARS);

    // 8. Run plugin post-hooks
    let additional_context = registry.on_prompt_post(&prompt, &context_output, &session_state);

    // 9. Save state
    let state_json = serde_json::to_string_pretty(&state)?;
    attentive_telemetry::atomic_write(&state_path, state_json.as_bytes())?;

    // 10. Write output to stdout
    let output = PromptOutput {
        context: if additional_context.is_empty() {
            context_output
        } else {
            format!("{}\n{}", context_output, additional_context)
        },
        metadata: serde_json::json!({
            "hot_count": hot_files.len(),
            "warm_count": warm_files.len(),
        }),
    };

    let output_json = serde_json::to_string(&output)?;
    io::stdout().write_all(output_json.as_bytes())?;
    io::stdout().flush()?;

    Ok(())
}

pub fn hook_session_start() -> anyhow::Result<()> {
    let paths = Paths::new()?;
    let project_dir = paths.project_dir()?;
    std::fs::create_dir_all(&project_dir)?;

    // 1. Detect project switch (legacy — less relevant with project-scoped state)
    let cwd = std::env::current_dir()?.to_string_lossy().to_lowercase();
    let session_state_path = paths.session_state_path()?;

    if detect_project_switch(&session_state_path, &cwd) {
        // Reset attention state
        let attn_path = paths.attn_state_path()?;
        if attn_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&attn_path) {
                if let Ok(mut state) = serde_json::from_str::<AttentionState>(&content) {
                    for score in state.scores.values_mut() {
                        *score = 0.0;
                    }
                    state.turn_count = 0;
                    if let Ok(json) = serde_json::to_string_pretty(&state) {
                        let _ = attentive_telemetry::atomic_write(&attn_path, json.as_bytes());
                    }
                }
            }
        }
        eprintln!("[attentive] Project switch detected, attention reset");
    }

    // 2. Initialize plugins
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(attentive_plugins::BurnRatePlugin::new()));
    registry.register(Box::new(attentive_plugins::LoopBreakerPlugin::new()));
    registry.register(Box::new(attentive_plugins::VerifyFirstPlugin::new()));

    let session_state = std::collections::HashMap::new();
    let messages = registry.on_session_start(&session_state);

    // 3. Dashboard
    let turns: Vec<attentive_telemetry::TurnRecord> =
        attentive_telemetry::read_jsonl(&paths.turns_file()).unwrap_or_default();
    let recent: Vec<_> = turns.into_iter().rev().take(100).collect();
    let dashboard = build_dashboard(&recent, None);
    if !dashboard.is_empty() {
        println!("{}", dashboard);
    }

    // 4. Write session state
    let session_state_file = paths.session_state_path()?;
    let session_data = serde_json::json!({
        "session_id": uuid_simple(),
        "started_at": chrono::Utc::now().to_rfc3339(),
        "plugin_messages": messages,
    });

    let json = serde_json::to_string_pretty(&session_data)?;
    attentive_telemetry::atomic_write(&session_state_file, json.as_bytes())?;

    // 5. Output plugin messages to stderr
    for msg in &messages {
        eprintln!("{}", msg);
    }

    Ok(())
}

pub fn hook_stop() -> anyhow::Result<()> {
    use attentive_telemetry::{append_jsonl, TurnRecord};

    // 1. Read stdin (tool calls JSON)
    let mut input_str = String::new();
    io::stdin().read_to_string(&mut input_str)?;

    let tool_calls: Vec<attentive_plugins::ToolCall> = if input_str.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&input_str).unwrap_or_default()
    };

    // 2. Initialize plugins and run on_stop
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(attentive_plugins::BurnRatePlugin::new()));
    registry.register(Box::new(attentive_plugins::LoopBreakerPlugin::new()));
    registry.register(Box::new(attentive_plugins::VerifyFirstPlugin::new()));

    let session_state = std::collections::HashMap::new();
    let messages = registry.on_stop(&tool_calls, &session_state);

    for msg in &messages {
        eprintln!("{}", msg);
    }

    // 3. Estimate tokens from attention state
    let paths = Paths::new()?;
    std::fs::create_dir_all(paths.telemetry_dir())?;
    let project_dir = paths.project_dir()?;
    std::fs::create_dir_all(&project_dir)?;

    let state_path = paths.attn_state_path()?;
    let (injected_tokens, used_tokens) = if state_path.exists() {
        let content = std::fs::read_to_string(&state_path).unwrap_or_default();
        if let Ok(state) = serde_json::from_str::<AttentionState>(&content) {
            let hot = state.get_hot_files();
            let warm = state.get_warm_files();
            // Rough estimate: HOT files ~500 tokens each, WARM ~200 each
            let injected = hot.len() * 500 + warm.len() * 200;
            // Used tokens estimated from tool calls
            let used = tool_calls
                .iter()
                .map(|tc| {
                    let content_len = tc.content.as_deref().unwrap_or("").len()
                        + tc.old_string.as_deref().unwrap_or("").len()
                        + tc.command.as_deref().unwrap_or("").len();
                    content_len / 4
                })
                .sum::<usize>();
            (injected, used)
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };

    let waste_ratio = calculate_waste_ratio(injected_tokens, used_tokens);

    let files_used = extract_files_from_tool_calls(&tool_calls);

    let files_injected = if state_path.exists() {
        let content = std::fs::read_to_string(&state_path).unwrap_or_default();
        if let Ok(state) = serde_json::from_str::<AttentionState>(&content) {
            let mut injected = state.get_hot_files();
            injected.extend(state.get_warm_files());
            injected
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let context_confidence = compute_context_confidence(&files_injected, &files_used);
    let injection_chars = injected_tokens * 4;

    let record = TurnRecord {
        turn_id: uuid_simple(),
        session_id: "default".to_string(),
        project: std::env::current_dir()?.to_string_lossy().to_string(),
        timestamp: chrono::Utc::now(),
        injected_tokens,
        used_tokens,
        waste_ratio,
        files_injected,
        files_used: files_used.clone(),
        was_notification: false,
        injection_chars,
        context_confidence: Some(context_confidence),
    };
    append_jsonl(&paths.turns_file(), &record)?;

    // Train learner with files_used
    let learned_state_path = paths.learned_state_path()?;
    if let Some(mut learner) = load_learner(&learned_state_path) {
        learner.observe_turn("", &files_used);
        if let Ok(json) = serde_json::to_string(&learner) {
            let _ = attentive_telemetry::atomic_write(&learned_state_path, json.as_bytes());
        }
    }

    Ok(())
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("turn_{:x}", nanos)
}

fn extract_files_from_tool_calls(tool_calls: &[attentive_plugins::ToolCall]) -> Vec<String> {
    let mut files = std::collections::HashSet::new();
    for tc in tool_calls {
        if let Some(target) = &tc.target {
            if !target.is_empty() {
                files.insert(target.clone());
            }
        }
    }
    files.into_iter().collect()
}

fn compute_context_confidence(files_injected: &[String], files_used: &[String]) -> f64 {
    if files_injected.is_empty() {
        return 0.0;
    }
    let used_set: std::collections::HashSet<&String> = files_used.iter().collect();
    let injected_set: std::collections::HashSet<&String> = files_injected.iter().collect();
    injected_set
        .iter()
        .filter(|f| used_set.contains(*f))
        .count() as f64
        / injected_set.len() as f64
}

fn calculate_waste_ratio(injected_tokens: usize, used_tokens: usize) -> f64 {
    if injected_tokens > 0 {
        (1.0 - (used_tokens as f64 / injected_tokens as f64)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_hook_session_start() {
        // Ensure home .claude directory exists for the test
        let paths = Paths::new().unwrap();
        std::fs::create_dir_all(&paths.home_claude).unwrap();

        let result = hook_session_start();
        if let Err(e) = &result {
            eprintln!("hook_session_start failed: {:?}", e);
        }
        assert!(result.is_ok(), "hook_session_start should succeed");

        let session_state_path = paths.session_state_path().unwrap();
        assert!(
            session_state_path.exists(),
            "session_state.json should be created"
        );

        let content = std::fs::read_to_string(&session_state_path).unwrap();
        assert!(
            content.contains("plugin_messages"),
            "session_state should contain plugin_messages"
        );
    }

    #[test]
    fn test_waste_ratio_calculation() {
        assert!((calculate_waste_ratio(1000, 300) - 0.7).abs() < f64::EPSILON);
        assert!((calculate_waste_ratio(1000, 1000) - 0.0).abs() < f64::EPSILON);
        assert!((calculate_waste_ratio(1000, 1500) - 0.0).abs() < f64::EPSILON); // clamped
        assert!((calculate_waste_ratio(0, 500) - 0.0).abs() < f64::EPSILON); // no injection
    }

    #[test]
    fn test_load_config_from_attentive_json() {
        let temp = tempfile::TempDir::new().unwrap();
        let claude_dir = temp.path();
        std::fs::create_dir_all(claude_dir).unwrap();

        let config_json = serde_json::json!({
            "co_activation": {
                "router.rs": ["config.rs"]
            },
            "pinned_files": ["important.md"],
            "demoted_files": ["old.md"]
        });
        std::fs::write(
            claude_dir.join("attentive.json"),
            serde_json::to_string_pretty(&config_json).unwrap(),
        )
        .unwrap();

        let config = load_config(claude_dir);
        assert_eq!(config.co_activation.len(), 1);
        assert_eq!(config.pinned_files, vec!["important.md"]);
        assert_eq!(config.demoted_files, vec!["old.md"]);
    }

    #[test]
    fn test_load_config_missing_file_returns_default() {
        let temp = tempfile::TempDir::new().unwrap();
        let config = load_config(temp.path());
        assert!(config.co_activation.is_empty());
        assert!(config.pinned_files.is_empty());
        assert!(config.demoted_files.is_empty());
    }

    #[test]
    fn test_load_learner_from_state() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut learner = attentive_learn::Learner::new();
        for _ in 0..30 {
            learner.observe_turn("router config", &["router.rs".to_string()]);
        }
        let json = serde_json::to_string(&learner).unwrap();
        let state_path = temp.path().join("learned_state.json");
        std::fs::write(&state_path, &json).unwrap();

        let loaded = load_learner(&state_path);
        assert!(loaded.is_some());
        let loaded_learner = loaded.unwrap();
        // Verify learner is Active (30 turns observed)
        assert_eq!(format!("{:?}", loaded_learner.maturity()), "Active");
    }

    #[test]
    fn test_detect_project_switch() {
        let temp = tempfile::TempDir::new().unwrap();
        let session_path = temp.path().join("session_state.json");

        // First call: no previous project
        let switched = detect_project_switch(&session_path, "/project/a");
        assert!(!switched); // No previous, so not a switch

        // Second call: same project
        let switched = detect_project_switch(&session_path, "/project/a");
        assert!(!switched);

        // Third call: different project
        let switched = detect_project_switch(&session_path, "/project/b");
        assert!(switched);
    }

    #[test]
    fn test_build_dashboard_empty() {
        let dashboard = build_dashboard(&[], None);
        assert!(dashboard.is_empty()); // No data = no dashboard
    }

    #[test]
    fn test_build_dashboard_with_turns() {
        let turns = vec![attentive_telemetry::TurnRecord {
            turn_id: "t1".to_string(),
            session_id: "s1".to_string(),
            project: "/test".to_string(),
            timestamp: chrono::Utc::now(),
            injected_tokens: 1000,
            used_tokens: 600,
            waste_ratio: 0.4,
            files_injected: vec!["a.rs".to_string()],
            files_used: vec!["a.rs".to_string()],
            was_notification: false,
            injection_chars: 4000,
            context_confidence: Some(0.8),
        }];
        let dashboard = build_dashboard(&turns, None);
        assert!(dashboard.contains("attentive"));
        assert!(dashboard.contains("Waste"));
    }

    #[test]
    fn test_extract_files_from_tool_calls() {
        let tool_calls = vec![
            attentive_plugins::ToolCall {
                tool: "Read".to_string(),
                target: Some("/path/to/router.rs".to_string()),
                content: None,
                old_string: None,
                command: None,
            },
            attentive_plugins::ToolCall {
                tool: "Edit".to_string(),
                target: Some("/path/to/config.rs".to_string()),
                content: Some("new content".to_string()),
                old_string: Some("old content".to_string()),
                command: None,
            },
            attentive_plugins::ToolCall {
                tool: "Bash".to_string(),
                target: None,
                content: None,
                old_string: None,
                command: Some("cargo test".to_string()),
            },
        ];

        let files_used = extract_files_from_tool_calls(&tool_calls);
        assert!(files_used.contains(&"/path/to/router.rs".to_string()));
        assert!(files_used.contains(&"/path/to/config.rs".to_string()));
        assert_eq!(files_used.len(), 2);
    }

    #[test]
    fn test_compute_context_confidence() {
        let files_injected = vec!["a.rs".to_string(), "b.rs".to_string(), "c.rs".to_string()];
        let files_used = vec!["a.rs".to_string(), "b.rs".to_string()];

        let confidence = compute_context_confidence(&files_injected, &files_used);
        assert!(confidence > 0.5);
        assert!(confidence < 1.0);
    }

    #[test]
    fn test_compute_context_confidence_empty() {
        let confidence = compute_context_confidence(&[], &[]);
        assert!((confidence - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_build_hot_content() {
        let temp = tempfile::TempDir::new().unwrap();
        let file_path = temp.path().join("test.md");
        std::fs::write(
            &file_path,
            "# Title\nSome content\n## Section\nMore content",
        )
        .unwrap();

        let content = read_file_content(file_path.to_str().unwrap(), 10000);
        assert!(content.contains("# Title"));
        assert!(content.contains("Some content"));
    }

    #[test]
    fn test_build_warm_toc() {
        let content = "# Main Title\nParagraph text here.\n## Section One\nDetails.\n### Subsection\nMore details.\nfn foo() {\n}\ndef bar():\n    pass";
        let toc = extract_toc(content);
        assert!(toc.contains("Main Title"));
        assert!(toc.contains("Section One"));
        assert!(toc.contains("Subsection"));
    }

    #[test]
    fn test_build_context_with_content() {
        let temp = tempfile::TempDir::new().unwrap();
        let hot_file = temp.path().join("hot.md");
        std::fs::write(&hot_file, "# Hot File\nImportant content here").unwrap();
        let warm_file = temp.path().join("warm.md");
        std::fs::write(
            &warm_file,
            "# Warm File\n## Section A\nDetails\n## Section B\nMore",
        )
        .unwrap();

        let hot_files = vec![hot_file.to_str().unwrap().to_string()];
        let warm_files = vec![warm_file.to_str().unwrap().to_string()];

        let context = build_tiered_context(&hot_files, &warm_files, 20000);
        assert!(context.contains("[HOT]"));
        assert!(context.contains("Important content here"));
        assert!(context.contains("[WARM]"));
        assert!(context.contains("Section A"));
    }

    #[test]
    fn test_max_chars_respected() {
        let temp = tempfile::TempDir::new().unwrap();
        let big_file = temp.path().join("big.md");
        let big_content = "x".repeat(50000);
        std::fs::write(&big_file, &big_content).unwrap();

        let content = read_file_content(big_file.to_str().unwrap(), 1000);
        assert!(content.len() <= 1100); // Allow small overhead for truncation marker
    }
}
