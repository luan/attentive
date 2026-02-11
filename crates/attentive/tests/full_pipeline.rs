use attentive_core::{AttentionState, Config, Router};
use attentive_learn::{Learner, Oracle, TaskType};
use attentive_plugins::{BurnRatePlugin, LoopBreakerPlugin, PluginRegistry, VerifyFirstPlugin};
use std::collections::HashMap;

#[test]
fn test_full_5_turn_pipeline() {
    // Setup
    let mut config = Config::new();
    config
        .co_activation
        .insert("router.rs".to_string(), vec!["config.rs".to_string()]);

    let router = Router::new(config);
    let mut state = AttentionState::new();
    let mut learner = Learner::new();

    // Seed files
    for f in ["router.rs", "config.rs", "utils.rs", "test.rs", "main.rs"] {
        state.scores.insert(f.to_string(), 0.5);
    }

    // Turn 1: Without keywords, no direct activation - only decay applies
    let activated = router.update_attention(
        &mut state,
        "fix the router",
        None,
        std::collections::HashSet::new(),
    );
    learner.observe_turn(
        "fix the router",
        &activated.iter().cloned().collect::<Vec<_>>(),
    );
    // All files decay from 0.5 to ~0.35 (0.5 * 0.7)
    assert!(*state.scores.get("router.rs").unwrap() < 0.5);
    assert!(*state.scores.get("router.rs").unwrap() > 0.3);

    // Turn 2: everything continues to decay
    let activated2 =
        router.update_attention(&mut state, "thanks", None, std::collections::HashSet::new());
    assert!(activated2.is_empty());
    assert!(*state.scores.get("router.rs").unwrap() < 0.35);

    // Turn 3-5: continued decay
    for turn in 3..=5 {
        router.update_attention(
            &mut state,
            "continuing work",
            None,
            std::collections::HashSet::new(),
        );
        let router_score = *state.scores.get("router.rs").unwrap();
        assert!(
            router_score < 0.5,
            "Turn {}: router.rs should decay below 0.5: {}",
            turn,
            router_score
        );
    }

    // After 5 turns of decay, unmentioned files should be COLD
    let utils_score = *state.scores.get("utils.rs").unwrap();
    assert!(
        utils_score < 0.25,
        "utils.rs should be COLD after 5 decay turns: {}",
        utils_score
    );

    // Verify tier output
    let (_hot, _warm, cold) = router.build_context_output(&state);
    assert!(
        cold.contains(&"utils.rs".to_string()),
        "utils.rs should be in cold tier"
    );
}

#[test]
fn test_oracle_classifies_turns() {
    let oracle = Oracle::new();
    assert_eq!(
        oracle.classify_task("fix the broken login"),
        TaskType::BugFix
    );
    assert_eq!(oracle.classify_task("add dark mode"), TaskType::Feature);
}

#[test]
fn test_plugin_registry_full_lifecycle() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(BurnRatePlugin::new()));
    registry.register(Box::new(LoopBreakerPlugin::new()));
    registry.register(Box::new(VerifyFirstPlugin::new()));

    let session_state = HashMap::new();
    let messages = registry.on_session_start(&session_state);
    assert!(
        messages.len() >= 2,
        "At least LoopBreaker and VerifyFirst should respond"
    );

    let (prompt, cont) = registry.on_prompt_pre("test".to_string(), &session_state);
    assert!(cont);
    assert!(!prompt.is_empty());

    let context = registry.on_prompt_post("test", "output", &session_state);
    assert!(!context.is_empty(), "Plugins should inject context");
}
