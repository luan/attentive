use attentive_core::{AttentionState, Config, Router};
use criterion::{criterion_group, criterion_main, Criterion};
use std::collections::HashMap;
use std::hint::black_box;

fn bench_router_update_10_files(c: &mut Criterion) {
    let config = Config::default();
    let router = Router::new(config);
    let mut state = AttentionState {
        scores: HashMap::new(),
        consecutive_turns: HashMap::new(),
        turn_count: 0,
    };

    for i in 0..10 {
        state.scores.insert(format!("file{}.rs", i), 0.5);
    }

    c.bench_function("router_update_10_files", |b| {
        b.iter(|| {
            let mut state_clone = state.clone();
            router.update_attention(&mut state_clone, black_box("test prompt"), None);
        });
    });
}

fn bench_router_co_activation_2hop(c: &mut Criterion) {
    let mut config = Config::default();
    config.co_activation.insert(
        "a.rs".to_string(),
        vec!["b.rs".to_string(), "c.rs".to_string()],
    );
    config
        .co_activation
        .insert("b.rs".to_string(), vec!["d.rs".to_string()]);

    let router = Router::new(config);
    let mut state = AttentionState {
        scores: HashMap::new(),
        consecutive_turns: HashMap::new(),
        turn_count: 0,
    };

    for f in ["a.rs", "b.rs", "c.rs", "d.rs"] {
        state.scores.insert(f.to_string(), 0.5);
    }

    c.bench_function("router_co_activation_2hop", |b| {
        b.iter(|| {
            let mut state_clone = state.clone();
            router.update_attention(&mut state_clone, black_box("key"), None);
        });
    });
}

criterion_group!(
    benches,
    bench_router_update_10_files,
    bench_router_co_activation_2hop
);
criterion_main!(benches);
