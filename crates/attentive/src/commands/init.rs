use attentive_telemetry::Paths;
use serde_json::Value;

pub fn run() -> anyhow::Result<()> {
    let paths = Paths::new()?;

    // Ensure ~/.claude exists
    if !paths.home_claude.exists() {
        anyhow::bail!(
            "~/.claude directory not found. Create it or ensure Claude Code is installed."
        );
    }

    // Read or create settings.json
    let settings_path = paths.home_claude.join("settings.json");
    let mut settings: Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::json!({})
    };

    // Ensure hooks object exists
    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }

    // Add attentive hooks for each event
    let hooks_to_add = vec![
        ("UserPromptSubmit", "attentive hook:user-prompt-submit"),
        ("SessionStart", "attentive hook:session-start"),
        ("Stop", "attentive hook:stop"),
    ];

    for (event_name, command) in hooks_to_add {
        add_hook_if_missing(&mut settings, event_name, command)?;
    }

    // Write back settings.json
    let json = serde_json::to_string_pretty(&settings)?;
    attentive_telemetry::atomic_write(&settings_path, json.as_bytes())?;

    println!("✓ Installed attentive hooks in ~/.claude/settings.json");
    println!("\nHooks added:");
    println!("  - UserPromptSubmit");
    println!("  - SessionStart");
    println!("  - Stop");

    Ok(())
}

fn add_hook_if_missing(
    settings: &mut Value,
    event_name: &str,
    command: &str,
) -> anyhow::Result<()> {
    let hooks = settings
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?;

    // Get or create the event array
    let event_array = hooks
        .entry(event_name)
        .or_insert_with(|| serde_json::json!([]));

    let event_groups = event_array
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("event {} is not an array", event_name))?;

    // Check if attentive hook already exists
    let already_exists = event_groups.iter().any(|group| {
        group
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|hooks_array| {
                hooks_array.iter().any(|hook| {
                    hook.get("command")
                        .and_then(|c| c.as_str())
                        .map(|cmd| cmd.starts_with("attentive "))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    });

    if !already_exists {
        // Add new hook group
        let new_group = serde_json::json!({
            "matcher": "",
            "hooks": [
                {
                    "type": "command",
                    "command": command
                }
            ]
        });
        event_groups.push(new_group);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_init_creates_hooks_in_global_settings() {
        let original_home = std::env::var("HOME").unwrap();
        let temp = TempDir::new().unwrap();
        let claude_dir = temp.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // Override HOME for this test
        unsafe { std::env::set_var("HOME", temp.path()) };

        let result = run();

        // Restore HOME
        unsafe { std::env::set_var("HOME", &original_home) };

        assert!(result.is_ok());
        assert!(claude_dir.join("settings.json").exists());

        let settings_content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        assert!(settings_content.contains("UserPromptSubmit"));
        assert!(settings_content.contains("SessionStart"));
        assert!(settings_content.contains("Stop"));
        assert!(settings_content.contains("attentive hook:user-prompt-submit"));
    }

    #[test]
    #[serial]
    fn test_init_preserves_existing_hooks() {
        let original_home = std::env::var("HOME").unwrap();
        let temp = TempDir::new().unwrap();
        let claude_dir = temp.path().join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // Create existing settings with custom hooks
        let existing_settings = serde_json::json!({
            "hooks": {
                "UserPromptSubmit": [
                    {
                        "matcher": "",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "custom-hook"
                            }
                        ]
                    }
                ]
            }
        });
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&existing_settings).unwrap(),
        )
        .unwrap();

        unsafe { std::env::set_var("HOME", temp.path()) };
        let result = run();
        unsafe { std::env::set_var("HOME", &original_home) };

        assert!(result.is_ok());

        let settings_content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
        assert!(settings_content.contains("custom-hook"));
        assert!(settings_content.contains("attentive hook:user-prompt-submit"));
    }

    #[test]
    fn test_add_hook_if_missing() {
        let mut settings = serde_json::json!({
            "hooks": {}
        });

        add_hook_if_missing(
            &mut settings,
            "UserPromptSubmit",
            "attentive hook:user-prompt-submit",
        )
        .unwrap();

        let hooks = settings["hooks"]["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(
            hooks[0]["hooks"][0]["command"].as_str().unwrap(),
            "attentive hook:user-prompt-submit"
        );
    }

    #[test]
    fn test_add_hook_skips_if_exists() {
        let mut settings = serde_json::json!({
            "hooks": {
                "UserPromptSubmit": [
                    {
                        "matcher": "",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "attentive hook:user-prompt-submit"
                            }
                        ]
                    }
                ]
            }
        });

        add_hook_if_missing(
            &mut settings,
            "UserPromptSubmit",
            "attentive hook:user-prompt-submit",
        )
        .unwrap();

        let hooks = settings["hooks"]["UserPromptSubmit"].as_array().unwrap();
        // Should still be 1, not duplicated
        assert_eq!(hooks.len(), 1);
    }
}
