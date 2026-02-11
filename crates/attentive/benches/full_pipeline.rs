use attentive_core::{AttentionState, Config, Router};
use attentive_plugins::{BurnRatePlugin, LoopBreakerPlugin, PluginRegistry, SessionState};
use criterion::{Criterion, criterion_group, criterion_main};
use std::collections::HashMap;
use std::hint::black_box;

fn bench_full_pipeline_20_files(c: &mut Criterion) {
    let config = Config::default();
    let router = Router::new(config);
    let mut state = AttentionState {
        scores: HashMap::new(),
        consecutive_turns: HashMap::new(),
        turn_count: 0,
    };

    for i in 0..20 {
        state.scores.insert(format!("file{}.rs", i), 0.5);
    }

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(LoopBreakerPlugin::new()));
    registry.register(Box::new(BurnRatePlugin::new()));

    let session_state = SessionState::new();

    c.bench_function("full_pipeline_20_files", |b| {
        b.iter(|| {
            let mut state_clone = state.clone();
            router.update_attention(&mut state_clone, black_box("test prompt"), None);
            registry.on_stop(&[], &session_state);
        });
    });
}

criterion_group!(benches, bench_full_pipeline_20_files);
criterion_main!(benches);
