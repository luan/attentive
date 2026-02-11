//! Base plugin trait and utilities

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Tool call representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub target: Option<String>,
    pub content: Option<String>,
    pub old_string: Option<String>,
    pub command: Option<String>,
}

/// Session state shared across plugins
pub type SessionState = HashMap<String, serde_json::Value>;

/// Get the plugins directory path
pub fn plugins_dir() -> anyhow::Result<PathBuf> {
    let paths = attentive_telemetry::Paths::new()?;
    Ok(paths.home_claude.join("plugins"))
}

/// Get state file path for a plugin
pub fn state_file(plugin_name: &str) -> anyhow::Result<PathBuf> {
    Ok(plugins_dir()?.join(format!("{}_state.json", plugin_name)))
}

/// Load plugin state from disk
pub fn load_state<T>(plugin_name: &str) -> anyhow::Result<T>
where
    T: for<'de> Deserialize<'de> + Default,
{
    let state_path = state_file(plugin_name)?;
    if !state_path.exists() {
        return Ok(T::default());
    }

    let contents = std::fs::read_to_string(&state_path)?;
    let state: T = serde_json::from_str(&contents)?;
    Ok(state)
}

/// Save plugin state to disk
pub fn save_state<T>(plugin_name: &str, state: &T) -> anyhow::Result<()>
where
    T: Serialize,
{
    let state_path = state_file(plugin_name)?;
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(state)?;
    attentive_telemetry::atomic_write(&state_path, json.as_bytes())?;
    Ok(())
}

/// Check if a plugin is enabled in config
pub fn is_plugin_enabled(plugin_name: &str) -> bool {
    let plugins_directory = match plugins_dir() {
        Ok(dir) => dir,
        Err(_) => return false, // Disabled when filesystem unavailable
    };

    let config_file = plugins_directory.join("config.json");

    if !config_file.exists() {
        return true; // Enabled by default
    }

    match std::fs::read_to_string(&config_file) {
        Ok(contents) => match serde_json::from_str::<serde_json::Value>(&contents) {
            Ok(config) => config
                .get("enabled")
                .and_then(|e| e.get(plugin_name))
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            Err(_) => true,
        },
        Err(_) => true,
    }
}

/// Base trait for attnroute plugins
pub trait Plugin: Send + Sync {
    /// Plugin name (unique identifier)
    fn name(&self) -> &str;

    /// Plugin version
    fn version(&self) -> &str {
        "0.1.0"
    }

    /// Plugin description
    fn description(&self) -> &str {
        ""
    }

    /// Check if plugin is enabled in config
    fn is_enabled(&self) -> bool {
        is_plugin_enabled(self.name())
    }

    // Lifecycle hooks (default implementations do nothing)

    /// Called on session start
    fn on_session_start(&mut self, _session_state: &SessionState) -> Option<String> {
        None
    }

    /// Called before context routing
    fn on_prompt_pre(&mut self, prompt: String, _session_state: &SessionState) -> (String, bool) {
        (prompt, true)
    }

    /// Called after context routing
    fn on_prompt_post(
        &mut self,
        _prompt: &str,
        _context_output: &str,
        _session_state: &SessionState,
    ) -> String {
        String::new()
    }

    /// Called after Claude finishes (Stop hook)
    fn on_stop(
        &mut self,
        _tool_calls: &[ToolCall],
        _session_state: &SessionState,
    ) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPlugin {
        name: String,
    }

    impl Plugin for MockPlugin {
        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn test_plugin_defaults() {
        let plugin = MockPlugin {
            name: "test-plugin".to_string(),
        };

        assert_eq!(plugin.name(), "test-plugin");
        assert_eq!(plugin.version(), "0.1.0");
        assert_eq!(plugin.description(), "");
        assert!(plugin.is_enabled()); // Default is enabled
    }

    #[test]
    fn test_state_file_path() {
        let path = state_file("test-plugin").unwrap();
        assert!(path.to_string_lossy().contains("plugins"));
        assert!(path.to_string_lossy().contains("test-plugin_state.json"));
    }

    #[test]
    fn test_load_save_state() {
        #[derive(Debug, serde::Serialize, serde::Deserialize, Default, PartialEq)]
        struct TestState {
            counter: i32,
            message: String,
        }

        let plugin_name = "test-state-plugin";

        // Save state
        let state = TestState {
            counter: 42,
            message: "hello".to_string(),
        };
        save_state(plugin_name, &state).unwrap();

        // Load state
        let loaded: TestState = load_state(plugin_name).unwrap();
        assert_eq!(loaded, state);

        // Cleanup
        std::fs::remove_file(state_file(plugin_name).unwrap()).ok();
    }

    #[test]
    fn test_lifecycle_hooks_default() {
        let mut plugin = MockPlugin {
            name: "test-hooks".to_string(),
        };

        let session_state = SessionState::new();

        // on_session_start returns None by default
        assert_eq!(plugin.on_session_start(&session_state), None);

        // on_prompt_pre passes through unchanged
        let (prompt, cont) = plugin.on_prompt_pre("test prompt".to_string(), &session_state);
        assert_eq!(prompt, "test prompt");
        assert!(cont);

        // on_prompt_post returns empty string by default
        let result = plugin.on_prompt_post("prompt", "context", &session_state);
        assert_eq!(result, "");

        // on_stop returns None by default
        assert_eq!(plugin.on_stop(&[], &session_state), None);
    }
}
