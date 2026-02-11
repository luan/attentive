mod common;

use attentive_core::Router;
use common::{sample_config, sample_state};

#[test]
fn test_cold_start_initialization() {
    let config = sample_config();
    let router = Router::new(config);
    let mut state = sample_state();

    state.scores.insert("router.rs".to_string(), 0.5);
    state.scores.insert("config.rs".to_string(), 0.5);

    let activated = router.update_attention(&mut state, "", None, std::collections::HashSet::new());

    // Verify decay applied (default decay rate is 0.7)
    assert!(
        state.scores["router.rs"] < 0.5,
        "router.rs score should decay"
    );
    assert!(
        state.scores["config.rs"] < 0.5,
        "config.rs score should decay"
    );
    assert!(
        activated.is_empty(),
        "No files should be activated with empty prompt"
    );
}

#[test]
fn test_cold_start_produces_valid_output() {
    let config = sample_config();
    let router = Router::new(config);
    let mut state = sample_state();

    state.scores.insert("hot.rs".to_string(), 0.9);
    state.scores.insert("warm.rs".to_string(), 0.5);
    state.scores.insert("cold.rs".to_string(), 0.1);

    router.update_attention(&mut state, "", None, std::collections::HashSet::new());
    let (hot, warm, cold) = router.build_context_output(&state);

    // Verify output structure is valid
    assert!(
        !hot.is_empty() || !warm.is_empty() || !cold.is_empty(),
        "Output should contain files"
    );

    // Hot files should have highest scores
    for file in &hot {
        let score = state.scores[file];
        assert!(score >= 0.8, "Hot files should have score >= 0.8");
    }
}
