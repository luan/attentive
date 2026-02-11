use attentive_core::{AttentionState, Config};
use std::collections::HashMap;

pub fn sample_config() -> Config {
    Config {
        decay_rates: attentive_core::DecayRates::default(),
        hot_threshold: 0.8,
        warm_threshold: 0.25,
        coactivation_boost: 0.35,
        transitive_boost: 0.15,
        max_hot_files: 10,
        max_warm_files: 20,
        pinned_floor_boost: 0.5,
        demoted_penalty: 0.3,
        co_activation: HashMap::new(),
        pinned_files: vec![],
        demoted_files: vec![],
    }
}

pub fn sample_state() -> AttentionState {
    AttentionState {
        scores: HashMap::new(),
        consecutive_turns: HashMap::new(),
        turn_count: 0,
    }
}
