use attentive_plugins::{BurnRatePlugin, LoopBreakerPlugin, Plugin, VerifyFirstPlugin};
use std::path::Path;

#[cfg(test)]
fn read_plugin_config(
    config_path: &Path,
) -> anyhow::Result<std::collections::HashMap<String, bool>> {
    if !config_path.exists() {
        return Ok(std::collections::HashMap::new());
    }
    let content = std::fs::read_to_string(config_path)?;
    let config: serde_json::Value = serde_json::from_str(&content)?;
    let enabled = config
        .get("enabled")
        .and_then(|e| e.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b)))
                .collect()
        })
        .unwrap_or_default();
    Ok(enabled)
}

fn set_plugin_enabled(config_path: &Path, name: &str, enabled: bool) -> anyhow::Result<()> {
    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(config_path)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::json!({})
    };

    if config.get("enabled").is_none() {
        config["enabled"] = serde_json::json!({});
    }
    config["enabled"][name] = serde_json::Value::Bool(enabled);

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&config)?;
    attentive_telemetry::atomic_write(config_path, json.as_bytes())?;
    Ok(())
}

pub fn run_list() -> anyhow::Result<()> {
    let plugins: Vec<Box<dyn Plugin>> = vec![
        Box::new(BurnRatePlugin::new()),
        Box::new(LoopBreakerPlugin::new()),
        Box::new(VerifyFirstPlugin::new()),
    ];

    println!("Registered Plugins");
    println!("==================");
    for plugin in &plugins {
        let status = if plugin.is_enabled() {
            "enabled"
        } else {
            "disabled"
        };
        println!("  {} v{} [{}]", plugin.name(), plugin.version(), status);
        let desc = plugin.description();
        if !desc.is_empty() {
            println!("    {}", desc);
        }
    }
    Ok(())
}

pub fn run_enable(name: &str) -> anyhow::Result<()> {
    let paths = attentive_telemetry::Paths::new()?;
    let config_path = paths.home_claude.join("plugins").join("config.json");
    set_plugin_enabled(&config_path, name, true)?;
    println!("Enabled plugin: {}", name);
    Ok(())
}

pub fn run_disable(name: &str) -> anyhow::Result<()> {
    let paths = attentive_telemetry::Paths::new()?;
    let config_path = paths.home_claude.join("plugins").join("config.json");
    set_plugin_enabled(&config_path, name, false)?;
    println!("Disabled plugin: {}", name);
    Ok(())
}

#[cfg(test)]
pub fn run() -> anyhow::Result<()> {
    run_list()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_plugins_list() {
        let paths = attentive_telemetry::Paths::new().unwrap();
        std::fs::create_dir_all(paths.home_claude.join("plugins")).unwrap();

        let result = run();
        assert!(result.is_ok());
    }

    #[test]
    fn test_enable_disable_plugin() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_path = temp.path().join("config.json");

        // Disable a plugin
        set_plugin_enabled(&config_path, "burnrate", false).unwrap();
        let config = read_plugin_config(&config_path).unwrap();
        assert_eq!(config.get("burnrate"), Some(&false));

        // Enable it back
        set_plugin_enabled(&config_path, "burnrate", true).unwrap();
        let config = read_plugin_config(&config_path).unwrap();
        assert_eq!(config.get("burnrate"), Some(&true));
    }

    #[test]
    fn test_enable_creates_config_if_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_path = temp.path().join("config.json");
        assert!(!config_path.exists());

        set_plugin_enabled(&config_path, "loopbreaker", false).unwrap();
        assert!(config_path.exists());
    }
}
