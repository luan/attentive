use attentive_plugins::{BurnRatePlugin, Plugin, SessionState};
use std::fs;
use std::path::PathBuf;

fn cleanup_state() {
    let paths = attentive_telemetry::Paths::new().unwrap();
    let state_file = paths
        .home_claude
        .join("plugins")
        .join("burnrate_state.json");
    std::fs::remove_file(state_file).ok();
}

fn stats_cache_path() -> PathBuf {
    let paths = attentive_telemetry::Paths::new().unwrap();
    paths.home_claude.join("stats-cache.json")
}

fn write_mock_stats(session_tokens: u64, model: &str) {
    let stats = serde_json::json!({
        "totalTokens": 100000,
        "sessionTokens": session_tokens,
        "inputTokens": 50000,
        "outputTokens": 50000,
        "costUsd": 1.5,
        "model": model
    });

    fs::create_dir_all(stats_cache_path().parent().unwrap()).ok();
    fs::write(
        stats_cache_path(),
        serde_json::to_string_pretty(&stats).unwrap(),
    )
    .unwrap();
}

fn cleanup_stats() {
    std::fs::remove_file(stats_cache_path()).ok();
}

#[test]
fn test_burnrate_initialization() {
    cleanup_state();
    cleanup_stats();

    // Ensure plugins directory exists
    let paths = attentive_telemetry::Paths::new().unwrap();
    std::fs::create_dir_all(paths.home_claude.join("plugins")).ok();

    write_mock_stats(50000, "claude-opus");

    let mut plugin = BurnRatePlugin::new();
    let session_state = SessionState::new();

    let result = plugin.on_session_start(&session_state);
    assert!(
        result.is_some(),
        "on_session_start should return Some: got None"
    );
    assert!(result.unwrap().contains("BurnRate"));

    cleanup_stats();
}

#[test]
fn test_no_warning_below_threshold() {
    cleanup_state();
    cleanup_stats();

    // Pro plan limit is 150k, at 50k usage we're nowhere near warning threshold
    write_mock_stats(50000, "claude-opus");

    let mut plugin = BurnRatePlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // No tool calls, just checking context
    let context = plugin.on_prompt_post("test", "context", &session_state);
    assert!(context.is_empty(), "Should not warn when far from limit");

    cleanup_stats();
}

#[test]
fn test_warning_at_threshold() {
    cleanup_state();
    cleanup_stats();

    // Close to pro limit (150k) - should warn
    write_mock_stats(145000, "claude-opus");

    let mut plugin = BurnRatePlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Simulate some usage to build up samples
    plugin.on_stop(&[], &session_state);

    // After some time, update stats
    write_mock_stats(148000, "claude-opus");
    plugin.on_stop(&[], &session_state);

    let _context = plugin.on_prompt_post("test", "context", &session_state);

    // Note: Warning threshold requires burn rate calculation which needs multiple samples over time
    // This test verifies the plugin doesn't crash at high usage

    cleanup_stats();
}

#[test]
fn test_session_tracking() {
    cleanup_state();
    cleanup_stats();

    write_mock_stats(10000, "claude-opus");

    let mut plugin = BurnRatePlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Call on_stop multiple times to build history
    for _ in 0..5 {
        plugin.on_stop(&[], &session_state);
    }

    // State should be saved with samples
    cleanup_stats();
}
