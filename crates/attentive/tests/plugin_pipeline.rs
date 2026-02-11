use attentive_plugins::{
    BurnRatePlugin, LoopBreakerPlugin, Plugin, PluginRegistry, SessionState, VerifyFirstPlugin,
};

#[test]
fn test_plugin_hooks_execute() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(BurnRatePlugin::new()));
    registry.register(Box::new(LoopBreakerPlugin::new()));

    let session_state = SessionState::new();

    let messages = registry.on_session_start(&session_state);

    assert!(
        !messages.is_empty(),
        "Plugins should return session start messages"
    );
}

#[test]
fn test_plugins_maintain_state() {
    let mut plugin = VerifyFirstPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Plugin state should persist across calls
    let result1 = plugin.on_stop(&[], &session_state);
    let result2 = plugin.on_stop(&[], &session_state);

    assert_eq!(result1, result2, "Plugin should maintain consistent state");
}
