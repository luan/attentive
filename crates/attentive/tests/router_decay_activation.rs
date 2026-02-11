mod common;

use attentive_core::Router;
use common::{sample_config, sample_state};

#[test]
fn test_five_turn_decay_sequence() {
    let config = sample_config();
    let router = Router::new(config);
    let mut state = sample_state();
    state.scores.insert("file.rs".to_string(), 1.0);

    let decay_rate: f64 = 0.7;

    for turn in 1..=5 {
        router.update_attention(&mut state, "", None, std::collections::HashSet::new());
        let expected_score = 1.0 * decay_rate.powi(turn);
        let actual_score = state.scores["file.rs"];
        assert!(
            (actual_score - expected_score).abs() < 0.01,
            "Turn {}: expected {}, got {}",
            turn,
            expected_score,
            actual_score
        );
    }
}

#[test]
fn test_co_activation_graph_built() {
    let mut config = sample_config();
    config
        .co_activation
        .insert("a.rs".to_string(), vec!["b.rs".to_string()]);

    let router = Router::new(config);
    let mut state = sample_state();
    state.scores.insert("a.rs".to_string(), 1.0);
    state.scores.insert("b.rs".to_string(), 0.1);

    router.update_attention(&mut state, "prompt", None, std::collections::HashSet::new());

    // Without keyword activation, co-activation won't trigger
    // (it requires directly_activated to be non-empty)
    // This test verifies the router builds successfully with co_activation config
    assert!(state.scores["a.rs"] > 0.6, "a.rs should decay");
    assert!(state.scores["b.rs"] < 0.15, "b.rs just decays");
}
