//! LoopBreaker Plugin - Detects and breaks repetitive failure loops

use crate::base::{Plugin, SessionState, ToolCall, load_state, save_state};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const LOOP_THRESHOLD: usize = 3;
const HISTORY_SIZE: usize = 20;
const WORK_TOOLS: &[&str] = &[
    "Edit",
    "Write",
    "edit",
    "write",
    "MultiEdit",
    "Bash",
    "bash",
];

#[derive(Debug, Serialize, Deserialize, Default)]
struct LoopState {
    recent_attempts: VecDeque<Attempt>,
    active_loop: Option<LoopInfo>,
    loops_detected: usize,
    loops_broken: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Attempt {
    file: String,
    tool: String,
    signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoopInfo {
    file: String,
    count: usize,
}

pub struct LoopBreakerPlugin {
    name: String,
}

impl LoopBreakerPlugin {
    pub fn new() -> Self {
        Self {
            name: "loopbreaker".to_string(),
        }
    }

    fn create_signature(tool_call: &ToolCall) -> String {
        let tool = &tool_call.tool;
        let target = tool_call.target.as_deref().unwrap_or("");

        // Normalize path
        let normalized_target = target.replace('\\', "/");

        // Extract identifiers from old_string for signature
        let old_sig = if let Some(old_string) = &tool_call.old_string {
            // Simple identifier extraction
            old_string
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|s| !s.is_empty())
                .take(5)
                .collect::<Vec<_>>()
                .join(":")
        } else {
            String::new()
        };

        // Extract command for bash tools
        let cmd_sig = if let Some(command) = &tool_call.command {
            command.split_whitespace().next().unwrap_or("").to_string()
        } else {
            String::new()
        };

        format!("{}|{}|{}|{}", tool, normalized_target, old_sig, cmd_sig)
    }

    fn is_work_tool(tool: &str) -> bool {
        WORK_TOOLS.contains(&tool)
    }

    fn extract_work_attempts(tool_calls: &[ToolCall]) -> Vec<Attempt> {
        tool_calls
            .iter()
            .filter(|tc| Self::is_work_tool(&tc.tool))
            .filter(|tc| tc.target.is_some())
            .map(|tc| Attempt {
                file: tc.target.clone().unwrap(),
                tool: tc.tool.clone(),
                signature: Self::create_signature(tc),
            })
            .collect()
    }

    fn detect_loop(recent_attempts: &VecDeque<Attempt>) -> Option<LoopInfo> {
        if recent_attempts.len() < LOOP_THRESHOLD {
            return None;
        }

        // Group by file
        let mut by_file: std::collections::HashMap<String, Vec<&Attempt>> =
            std::collections::HashMap::new();
        for attempt in recent_attempts {
            by_file
                .entry(attempt.file.clone())
                .or_default()
                .push(attempt);
        }

        // Check each file for loops
        for (file, attempts) in by_file {
            if attempts.len() < LOOP_THRESHOLD {
                continue;
            }

            // Check recent attempts for similarity
            let recent: Vec<_> = attempts.iter().rev().take(LOOP_THRESHOLD).collect();

            // Count signatures
            let mut sig_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for attempt in &recent {
                *sig_counts.entry(attempt.signature.clone()).or_default() += 1;
            }

            let max_count = sig_counts.values().max().copied().unwrap_or(0);

            if max_count >= LOOP_THRESHOLD {
                return Some(LoopInfo {
                    file,
                    count: max_count,
                });
            }
        }

        None
    }
}

impl Default for LoopBreakerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for LoopBreakerPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_session_start(&mut self, _session_state: &SessionState) -> Option<String> {
        let state = LoopState::default();
        save_state(self.name(), &state).ok();
        Some("LoopBreaker: Active (repetitive failure detection)".to_string())
    }

    fn on_prompt_post(
        &mut self,
        _prompt: &str,
        _context_output: &str,
        _session_state: &SessionState,
    ) -> String {
        let state: LoopState = load_state(self.name()).unwrap_or_default();

        if let Some(loop_info) = &state.active_loop {
            let file_name = std::path::Path::new(&loop_info.file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            format!(
                "\n## LoopBreaker Alert\n\
                **WARNING:** You've attempted to modify `{}` {} times with similar approach.\n\
                \n\
                **STOP and reconsider your approach:**\n\
                1. Re-read the file to verify your understanding\n\
                2. Check if you're solving the RIGHT problem\n\
                3. Consider a completely different approach\n\
                4. If stuck, ask the user for clarification\n\
                \n\
                **Do NOT repeat the same fix.** Try something fundamentally different.\n",
                file_name, loop_info.count
            )
        } else {
            String::new()
        }
    }

    fn on_stop(
        &mut self,
        tool_calls: &[ToolCall],
        _session_state: &SessionState,
    ) -> Option<String> {
        let mut state: LoopState = load_state(self.name()).unwrap_or_default();

        if tool_calls.is_empty() {
            // No tool calls - clear active loop
            if state.active_loop.is_some() {
                state.active_loop = None;
                state.loops_broken += 1;
                save_state(self.name(), &state).ok();
            }
            return None;
        }

        // Extract work attempts
        let work_attempts = Self::extract_work_attempts(tool_calls);

        if work_attempts.is_empty() {
            // No work tools - clear active loop
            if state.active_loop.is_some() {
                state.active_loop = None;
                state.loops_broken += 1;
                save_state(self.name(), &state).ok();
            }
            return None;
        }

        // Check if working on different file than active loop
        if let Some(active_loop) = &state.active_loop {
            let current_files: std::collections::HashSet<_> =
                work_attempts.iter().map(|a| &a.file).collect();
            if !current_files.contains(&active_loop.file) {
                // Working on different file - break the loop
                state.active_loop = None;
                state.loops_broken += 1;
                save_state(self.name(), &state).ok();
                return None;
            }
        }

        // Add new attempts to history
        for attempt in work_attempts {
            state.recent_attempts.push_back(attempt);
        }

        // Trim to history size
        while state.recent_attempts.len() > HISTORY_SIZE {
            state.recent_attempts.pop_front();
        }

        // Detect loops
        if let Some(loop_info) = Self::detect_loop(&state.recent_attempts) {
            // Loop detected
            let is_new_loop = state
                .active_loop
                .as_ref()
                .is_none_or(|l| l.file != loop_info.file);

            if is_new_loop {
                state.loops_detected += 1;
            }

            let file_name = std::path::Path::new(&loop_info.file)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            state.active_loop = Some(loop_info.clone());
            save_state(self.name(), &state).ok();

            Some(format!(
                "[LoopBreaker] Detected {} similar attempts on {}",
                loop_info.count, file_name
            ))
        } else {
            // No loop - clear active loop if we had one
            if state.active_loop.is_some() {
                state.loops_broken += 1;
                state.active_loop = None;
            }
            save_state(self.name(), &state).ok();
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_creation() {
        let tool_call = ToolCall {
            tool: "Edit".to_string(),
            target: Some("/path/to/file.rs".to_string()),
            content: None,
            old_string: Some("fn test_function".to_string()),
            command: None,
        };

        let sig = LoopBreakerPlugin::create_signature(&tool_call);
        assert!(sig.contains("Edit"));
        assert!(sig.contains("/path/to/file.rs"));
        // Signature format: tool|path|identifiers|command
        // "fn test_function" splits to "fn", "test", "function"
        assert!(sig.contains("fn"), "Signature: {}", sig);
        assert!(sig.contains("test"), "Signature: {}", sig);
        assert!(sig.contains("function"), "Signature: {}", sig);
    }

    #[test]
    fn test_is_work_tool() {
        assert!(LoopBreakerPlugin::is_work_tool("Edit"));
        assert!(LoopBreakerPlugin::is_work_tool("Write"));
        assert!(LoopBreakerPlugin::is_work_tool("Bash"));
        assert!(!LoopBreakerPlugin::is_work_tool("Read"));
        assert!(!LoopBreakerPlugin::is_work_tool("Glob"));
    }
}
