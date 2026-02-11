//! VerifyFirst Plugin - Ensures files are read before being edited

use crate::base::{Plugin, SessionState, ToolCall, load_state, save_state};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const READ_TOOLS: &[&str] = &["Read", "read"];
const WRITE_TOOLS: &[&str] = &["Edit", "Write", "edit", "write", "MultiEdit"];
const MAX_DISPLAY_FILES: usize = 30;

#[derive(Debug, Serialize, Deserialize, Default)]
struct VerifyState {
    files_read: HashSet<String>,
    files_written: HashSet<String>,
    violations: Vec<Violation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Violation {
    file: String,
    tool: String,
}

pub struct VerifyFirstPlugin {
    name: String,
}

impl VerifyFirstPlugin {
    pub fn new() -> Self {
        Self {
            name: "verifyfirst".to_string(),
        }
    }

    fn normalize_path(path: &str) -> String {
        let normalized = path.replace('\\', "/");
        #[cfg(target_os = "windows")]
        {
            normalized.to_lowercase()
        }
        #[cfg(not(target_os = "windows"))]
        {
            normalized
        }
    }

    fn is_read_tool(tool: &str) -> bool {
        READ_TOOLS.contains(&tool)
    }

    fn is_write_tool(tool: &str) -> bool {
        WRITE_TOOLS.contains(&tool)
    }
}

impl Default for VerifyFirstPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for VerifyFirstPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_session_start(&mut self, _session_state: &SessionState) -> Option<String> {
        let state = VerifyState::default();
        save_state(self.name(), &state).ok()?;
        Some("VerifyFirst: Active (read-before-write policy)".to_string())
    }

    fn on_prompt_post(
        &mut self,
        _prompt: &str,
        _context_output: &str,
        _session_state: &SessionState,
    ) -> String {
        let state: VerifyState = load_state(self.name()).unwrap_or_default();

        let mut policy_lines = vec![
            String::new(),
            "## VerifyFirst Policy".to_string(),
            "You MUST read a file before editing it. This ensures you understand the full context."
                .to_string(),
            String::new(),
        ];

        if !state.files_read.is_empty() {
            let files: Vec<_> = state.files_read.iter().take(MAX_DISPLAY_FILES).collect();

            policy_lines.push("**Files verified (safe to edit):**".to_string());
            for file in &files {
                let name = std::path::Path::new(file)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(file);
                policy_lines.push(format!("- `{}`", name));
            }

            if state.files_read.len() > MAX_DISPLAY_FILES {
                policy_lines.push(format!(
                    "- ... and {} more",
                    state.files_read.len() - MAX_DISPLAY_FILES
                ));
            }

            policy_lines.push(String::new());
            policy_lines.push(
                "**IMPORTANT:** For any file NOT in this list, you MUST use Read first."
                    .to_string(),
            );
        } else {
            policy_lines.push("**No files have been read yet this session.**".to_string());
            policy_lines
                .push("You MUST Read any file before attempting to Edit or Write it.".to_string());
        }

        policy_lines.push(String::new());
        policy_lines.join("\n")
    }

    fn on_stop(
        &mut self,
        tool_calls: &[ToolCall],
        _session_state: &SessionState,
    ) -> Option<String> {
        if tool_calls.is_empty() {
            return None;
        }

        let mut state: VerifyState = load_state(self.name()).unwrap_or_default();
        let mut new_violations = Vec::new();

        for tc in tool_calls {
            let tool = &tc.tool;
            let target = match tc.target.as_deref() {
                Some(t) => t,
                None => continue,
            };
            let normalized = Self::normalize_path(target);

            if Self::is_read_tool(tool) {
                state.files_read.insert(normalized);
            } else if Self::is_write_tool(tool) {
                state.files_written.insert(normalized.clone());

                if !state.files_read.contains(&normalized) {
                    let violation = Violation {
                        file: target.to_string(),
                        tool: tool.clone(),
                    };
                    state.violations.push(violation.clone());
                    new_violations.push(violation);
                }
            }
        }

        save_state(self.name(), &state).ok();

        if !new_violations.is_empty() {
            let files: Vec<_> = new_violations.iter().map(|v| v.file.as_str()).collect();
            Some(format!(
                "[VerifyFirst] VIOLATION: Edited without reading first: {}",
                files.join(", ")
            ))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        let path1 = "/path/to/file.rs";
        let path2 = "/path/to/file.rs";
        assert_eq!(
            VerifyFirstPlugin::normalize_path(path1),
            VerifyFirstPlugin::normalize_path(path2)
        );
    }

    #[test]
    fn test_tool_classification() {
        assert!(VerifyFirstPlugin::is_read_tool("Read"));
        assert!(VerifyFirstPlugin::is_read_tool("read"));
        assert!(!VerifyFirstPlugin::is_read_tool("Edit"));

        assert!(VerifyFirstPlugin::is_write_tool("Edit"));
        assert!(VerifyFirstPlugin::is_write_tool("Write"));
        assert!(VerifyFirstPlugin::is_write_tool("write"));
        assert!(!VerifyFirstPlugin::is_write_tool("Read"));
    }
}
