use attentive_plugins::{LoopBreakerPlugin, Plugin, SessionState, ToolCall};
use serial_test::serial;

fn cleanup_state() {
    let paths = attentive_telemetry::Paths::new().unwrap();
    let state_file = paths
        .home_claude
        .join("plugins")
        .join("loopbreaker_state.json");
    std::fs::remove_file(state_file).ok();
}

#[test]
#[serial]
fn test_three_identical_signatures_detects_loop() {
    cleanup_state();
    let mut plugin = LoopBreakerPlugin::new();

    // Initialize session to clear any stale state
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // First attempt - no loop
    let tool_calls = vec![ToolCall {
        tool: "Edit".to_string(),
        target: Some("/path/to/file.rs".to_string()),
        content: Some("new content".to_string()),
        old_string: Some("old content".to_string()),
        command: None,
    }];

    let result1 = plugin.on_stop(&tool_calls, &session_state);
    assert!(result1.is_none(), "First attempt should not trigger loop");

    // Second identical attempt - no loop yet
    let result2 = plugin.on_stop(&tool_calls, &session_state);
    assert!(result2.is_none(), "Second attempt should not trigger loop");

    // Third identical attempt - loop detected!
    let result3 = plugin.on_stop(&tool_calls, &session_state);
    assert!(
        result3.is_some(),
        "Third identical attempt should detect loop"
    );
    assert!(result3.unwrap().contains("similar attempts"));
}

#[test]
#[serial]
fn test_different_files_no_loop() {
    cleanup_state();
    let mut plugin = LoopBreakerPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Edit file 1
    let tool_calls_1 = vec![ToolCall {
        tool: "Edit".to_string(),
        target: Some("/path/to/file1.rs".to_string()),
        content: None,
        old_string: Some("content".to_string()),
        command: None,
    }];
    plugin.on_stop(&tool_calls_1, &session_state);

    // Edit file 2
    let tool_calls_2 = vec![ToolCall {
        tool: "Edit".to_string(),
        target: Some("/path/to/file2.rs".to_string()),
        content: None,
        old_string: Some("content".to_string()),
        command: None,
    }];
    plugin.on_stop(&tool_calls_2, &session_state);

    // Edit file 3
    let tool_calls_3 = vec![ToolCall {
        tool: "Edit".to_string(),
        target: Some("/path/to/file3.rs".to_string()),
        content: None,
        old_string: Some("content".to_string()),
        command: None,
    }];
    let result = plugin.on_stop(&tool_calls_3, &session_state);

    assert!(result.is_none(), "Different files should not trigger loop");
}

#[test]
#[serial]
fn test_read_tools_dont_count_as_work() {
    cleanup_state();
    let mut plugin = LoopBreakerPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Repeated reads should not trigger loop
    let read_calls = vec![ToolCall {
        tool: "Read".to_string(),
        target: Some("/path/to/file.rs".to_string()),
        content: None,
        old_string: None,
        command: None,
    }];

    plugin.on_stop(&read_calls, &session_state);
    plugin.on_stop(&read_calls, &session_state);
    let result = plugin.on_stop(&read_calls, &session_state);

    assert!(result.is_none(), "Read tools should not count as loop");
}

#[test]
#[serial]
fn test_loop_broken_by_different_file() {
    cleanup_state();
    let mut plugin = LoopBreakerPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    let file1_calls = vec![ToolCall {
        tool: "Edit".to_string(),
        target: Some("/path/to/file1.rs".to_string()),
        content: None,
        old_string: Some("content".to_string()),
        command: None,
    }];

    let file2_calls = vec![ToolCall {
        tool: "Edit".to_string(),
        target: Some("/path/to/file2.rs".to_string()),
        content: None,
        old_string: Some("content".to_string()),
        command: None,
    }];

    // Build up a loop on file1
    plugin.on_stop(&file1_calls, &session_state);
    plugin.on_stop(&file1_calls, &session_state);
    let result = plugin.on_stop(&file1_calls, &session_state);
    assert!(result.is_some(), "Loop should be detected");

    // Work on different file should break the loop
    plugin.on_stop(&file2_calls, &session_state);

    // Back to file1 - loop should be reset
    plugin.on_stop(&file1_calls, &session_state);
    plugin.on_stop(&file1_calls, &session_state);
    let result2 = plugin.on_stop(&file1_calls, &session_state);

    // Loop should be detected again after 3 attempts
    assert!(
        result2.is_some(),
        "Loop should be detected again after reset"
    );
}
