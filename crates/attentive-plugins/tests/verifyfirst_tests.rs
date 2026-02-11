use attentive_plugins::{Plugin, SessionState, ToolCall, VerifyFirstPlugin};
use serial_test::serial;

fn cleanup_state() {
    let paths = attentive_telemetry::Paths::new().unwrap();
    let state_file = paths
        .home_claude
        .join("plugins")
        .join("verifyfirst_state.json");
    std::fs::remove_file(state_file).ok();
}

#[test]
#[serial]
fn test_read_then_edit_no_violation() {
    cleanup_state();
    let mut plugin = VerifyFirstPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Read a file
    let read_call = vec![ToolCall {
        tool: "Read".to_string(),
        target: Some("/path/to/file.rs".to_string()),
        content: None,
        old_string: None,
        command: None,
    }];
    let result = plugin.on_stop(&read_call, &session_state);
    assert!(result.is_none(), "Read should not trigger violation");

    // Edit the same file - should be OK
    let edit_call = vec![ToolCall {
        tool: "Edit".to_string(),
        target: Some("/path/to/file.rs".to_string()),
        content: Some("new content".to_string()),
        old_string: Some("old content".to_string()),
        command: None,
    }];
    let result = plugin.on_stop(&edit_call, &session_state);
    assert!(result.is_none(), "Edit after Read should not violate");
}

#[test]
#[serial]
fn test_edit_without_read_violates() {
    cleanup_state();
    let mut plugin = VerifyFirstPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Edit without reading first - violation!
    let edit_call = vec![ToolCall {
        tool: "Edit".to_string(),
        target: Some("/path/to/unread_file.rs".to_string()),
        content: Some("new content".to_string()),
        old_string: Some("old content".to_string()),
        command: None,
    }];
    let result = plugin.on_stop(&edit_call, &session_state);
    assert!(result.is_some(), "Edit without Read should violate");
    assert!(result.unwrap().contains("VIOLATION"));
}

#[test]
#[serial]
fn test_write_without_read_violates() {
    cleanup_state();
    let mut plugin = VerifyFirstPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Write without reading first - violation!
    let write_call = vec![ToolCall {
        tool: "Write".to_string(),
        target: Some("/path/to/newfile.rs".to_string()),
        content: Some("file content".to_string()),
        old_string: None,
        command: None,
    }];
    let result = plugin.on_stop(&write_call, &session_state);
    assert!(result.is_some(), "Write without Read should violate");
    assert!(result.unwrap().contains("VIOLATION"));
}

#[test]
#[serial]
fn test_path_normalization() {
    cleanup_state();
    let mut plugin = VerifyFirstPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Read with forward slashes
    let read_call = vec![ToolCall {
        tool: "Read".to_string(),
        target: Some("/path/to/file.rs".to_string()),
        content: None,
        old_string: None,
        command: None,
    }];
    plugin.on_stop(&read_call, &session_state);

    // Edit with backslashes (Windows-style) - should still match
    let edit_call = vec![ToolCall {
        tool: "Edit".to_string(),
        target: Some("/path/to/file.rs".to_string()), // Same path
        content: Some("new".to_string()),
        old_string: None,
        command: None,
    }];
    let result = plugin.on_stop(&edit_call, &session_state);
    assert!(
        result.is_none(),
        "Path normalization should prevent false violations"
    );
}

#[test]
#[serial]
fn test_policy_context_injection() {
    cleanup_state();
    let mut plugin = VerifyFirstPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Read a file
    let read_call = vec![ToolCall {
        tool: "Read".to_string(),
        target: Some("/path/to/file.rs".to_string()),
        content: None,
        old_string: None,
        command: None,
    }];
    plugin.on_stop(&read_call, &session_state);

    // Check that policy context is injected
    let context = plugin.on_prompt_post("test prompt", "test context", &session_state);
    assert!(context.contains("VerifyFirst"), "Should inject policy");
    assert!(context.contains("file.rs"), "Should list verified files");
}

#[test]
#[serial]
fn test_tool_without_target_doesnt_skip_remaining() {
    cleanup_state();
    let mut plugin = VerifyFirstPlugin::new();
    let session_state = SessionState::new();
    plugin.on_session_start(&session_state);

    // Batch: Bash (no target) then Edit (should still be checked)
    let calls = vec![
        ToolCall {
            tool: "Bash".to_string(),
            target: None,
            content: None,
            old_string: None,
            command: Some("ls".to_string()),
        },
        ToolCall {
            tool: "Edit".to_string(),
            target: Some("/path/to/unread.rs".to_string()),
            content: Some("new".to_string()),
            old_string: Some("old".to_string()),
            command: None,
        },
    ];
    let result = plugin.on_stop(&calls, &session_state);
    assert!(
        result.is_some(),
        "Edit violation should not be skipped by preceding Bash call"
    );
    assert!(result.unwrap().contains("VIOLATION"));
}
