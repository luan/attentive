//! Plugin registry for loading and managing plugins

use crate::base::{Plugin, SessionState, ToolCall};

/// Registry for managing multiple plugins
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Register a plugin
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        if plugin.is_enabled() {
            self.plugins.push(plugin);
        }
    }

    /// Call on_session_start for all plugins
    pub fn on_session_start(&mut self, session_state: &SessionState) -> Vec<String> {
        self.plugins
            .iter_mut()
            .filter_map(|p| p.on_session_start(session_state))
            .collect()
    }

    /// Call on_prompt_pre for all plugins
    pub fn on_prompt_pre(
        &mut self,
        mut prompt: String,
        session_state: &SessionState,
    ) -> (String, bool) {
        for plugin in &mut self.plugins {
            let (new_prompt, should_continue) = plugin.on_prompt_pre(prompt, session_state);
            prompt = new_prompt;
            if !should_continue {
                return (prompt, false);
            }
        }
        (prompt, true)
    }

    /// Call on_prompt_post for all plugins
    pub fn on_prompt_post(
        &mut self,
        prompt: &str,
        context_output: &str,
        session_state: &SessionState,
    ) -> String {
        let mut additional_context = Vec::new();
        for plugin in &mut self.plugins {
            let context = plugin.on_prompt_post(prompt, context_output, session_state);
            if !context.is_empty() {
                additional_context.push(context);
            }
        }
        additional_context.join("\n")
    }

    /// Call on_stop for all plugins
    pub fn on_stop(
        &mut self,
        tool_calls: &[ToolCall],
        session_state: &SessionState,
    ) -> Vec<String> {
        self.plugins
            .iter_mut()
            .filter_map(|p| p.on_stop(tool_calls, session_state))
            .collect()
    }

    /// Get number of registered plugins
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct TestPlugin {
        name: String,
        enabled: bool,
        session_msg: Option<String>,
        stop_msg: Option<String>,
    }

    impl Plugin for TestPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn is_enabled(&self) -> bool {
            self.enabled
        }

        fn on_session_start(&mut self, _session_state: &SessionState) -> Option<String> {
            self.session_msg.clone()
        }

        fn on_prompt_pre(
            &mut self,
            prompt: String,
            _session_state: &SessionState,
        ) -> (String, bool) {
            (format!("[{}] {}", self.name, prompt), true)
        }

        fn on_prompt_post(
            &mut self,
            _prompt: &str,
            _context_output: &str,
            _session_state: &SessionState,
        ) -> String {
            format!("Context from {}", self.name)
        }

        fn on_stop(
            &mut self,
            _tool_calls: &[ToolCall],
            _session_state: &SessionState,
        ) -> Option<String> {
            self.stop_msg.clone()
        }
    }

    #[test]
    fn test_registry_register() {
        let mut registry = PluginRegistry::new();
        assert_eq!(registry.len(), 0);

        let plugin = Box::new(TestPlugin {
            name: "test1".to_string(),
            enabled: true,
            session_msg: None,
            stop_msg: None,
        });

        registry.register(plugin);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_registry_disabled_plugin_not_registered() {
        let mut registry = PluginRegistry::new();

        let plugin = Box::new(TestPlugin {
            name: "test-disabled".to_string(),
            enabled: false,
            session_msg: None,
            stop_msg: None,
        });

        registry.register(plugin);
        assert_eq!(registry.len(), 0); // Disabled plugin not added
    }

    #[test]
    fn test_registry_on_session_start() {
        let mut registry = PluginRegistry::new();

        registry.register(Box::new(TestPlugin {
            name: "test1".to_string(),
            enabled: true,
            session_msg: Some("Plugin 1 started".to_string()),
            stop_msg: None,
        }));

        registry.register(Box::new(TestPlugin {
            name: "test2".to_string(),
            enabled: true,
            session_msg: Some("Plugin 2 started".to_string()),
            stop_msg: None,
        }));

        let session_state = HashMap::new();
        let messages = registry.on_session_start(&session_state);

        assert_eq!(messages.len(), 2);
        assert!(messages.contains(&"Plugin 1 started".to_string()));
        assert!(messages.contains(&"Plugin 2 started".to_string()));
    }

    #[test]
    fn test_registry_on_prompt_pre_chains() {
        let mut registry = PluginRegistry::new();

        registry.register(Box::new(TestPlugin {
            name: "plugin1".to_string(),
            enabled: true,
            session_msg: None,
            stop_msg: None,
        }));

        registry.register(Box::new(TestPlugin {
            name: "plugin2".to_string(),
            enabled: true,
            session_msg: None,
            stop_msg: None,
        }));

        let session_state = HashMap::new();
        let (prompt, cont) = registry.on_prompt_pre("test".to_string(), &session_state);

        assert!(cont);
        assert_eq!(prompt, "[plugin2] [plugin1] test");
    }

    #[test]
    fn test_registry_on_prompt_post_concatenates() {
        let mut registry = PluginRegistry::new();

        registry.register(Box::new(TestPlugin {
            name: "plugin1".to_string(),
            enabled: true,
            session_msg: None,
            stop_msg: None,
        }));

        registry.register(Box::new(TestPlugin {
            name: "plugin2".to_string(),
            enabled: true,
            session_msg: None,
            stop_msg: None,
        }));

        let session_state = HashMap::new();
        let context = registry.on_prompt_post("prompt", "context", &session_state);

        assert!(context.contains("Context from plugin1"));
        assert!(context.contains("Context from plugin2"));
    }

    #[test]
    fn test_registry_on_stop() {
        let mut registry = PluginRegistry::new();

        registry.register(Box::new(TestPlugin {
            name: "test1".to_string(),
            enabled: true,
            session_msg: None,
            stop_msg: Some("Stop message 1".to_string()),
        }));

        registry.register(Box::new(TestPlugin {
            name: "test2".to_string(),
            enabled: true,
            session_msg: None,
            stop_msg: Some("Stop message 2".to_string()),
        }));

        let session_state = HashMap::new();
        let messages = registry.on_stop(&[], &session_state);

        assert_eq!(messages.len(), 2);
        assert!(messages.contains(&"Stop message 1".to_string()));
        assert!(messages.contains(&"Stop message 2".to_string()));
    }
}
