//! Path resolution for telemetry files

use std::path::PathBuf;
use std::process::Command;

/// Resolves standard paths for telemetry files
#[derive(Debug, Clone)]
pub struct Paths {
    pub home_claude: PathBuf,
    pub git_common_dir: Option<PathBuf>,
}

impl Paths {
    /// Create a new Paths resolver for the current working directory
    pub fn new() -> std::io::Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "home directory not found")
        })?;

        let home_claude = home.join(".claude");

        // Detect git worktree common dir
        let git_common_dir = detect_git_common_dir();

        Ok(Self {
            home_claude,
            git_common_dir,
        })
    }

    /// Get telemetry directory path
    pub fn telemetry_dir(&self) -> PathBuf {
        self.home_claude.join("telemetry")
    }

    /// Get turns.jsonl path
    pub fn turns_file(&self) -> PathBuf {
        self.telemetry_dir().join("turns.jsonl")
    }

    /// Get project-scoped directory based on current working directory
    pub fn project_dir(&self) -> std::io::Result<PathBuf> {
        let cwd = std::env::current_dir()?;
        let hash = cwd.to_string_lossy().replace(['/', '.'], "-");
        Ok(self.home_claude.join("projects").join(hash))
    }

    /// Get learned_state.json path for current project
    pub fn learned_state_path(&self) -> std::io::Result<PathBuf> {
        Ok(self.project_dir()?.join("learned_state.json"))
    }

    /// Get attn_state.json path for current project
    pub fn attn_state_path(&self) -> std::io::Result<PathBuf> {
        Ok(self.project_dir()?.join("attn_state.json"))
    }

    /// Get session_state.json path for current project
    pub fn session_state_path(&self) -> std::io::Result<PathBuf> {
        Ok(self.project_dir()?.join("session_state.json"))
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new().expect("HOME directory must be set")
    }
}

fn detect_git_common_dir() -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .ok()?;

    if output.status.success() {
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(PathBuf::from(path_str))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_new() {
        let paths = Paths::new().unwrap();
        assert!(paths.home_claude.ends_with(".claude"));
    }

    #[test]
    fn test_telemetry_dir() {
        let paths = Paths::new().unwrap();
        let telemetry = paths.telemetry_dir();
        assert!(telemetry.ends_with(".claude/telemetry"));
    }

    #[test]
    fn test_turns_file() {
        let paths = Paths::new().unwrap();
        let turns = paths.turns_file();
        assert!(turns.ends_with("turns.jsonl"));
    }

    #[test]
    fn test_project_dir() {
        let paths = Paths::new().unwrap();
        let project_dir = paths.project_dir().unwrap();
        assert!(project_dir.to_string_lossy().contains("projects"));
        // Should contain a hash based on CWD with slashes replaced by dashes
        let cwd = std::env::current_dir().unwrap();
        let expected_hash = cwd.to_string_lossy().replace(['/', '.'], "-");
        assert!(project_dir.ends_with(&expected_hash));
    }

    #[test]
    fn test_learned_state_path() {
        let paths = Paths::new().unwrap();
        let state_path = paths.learned_state_path().unwrap();
        assert!(state_path.ends_with("learned_state.json"));
        assert!(state_path.to_string_lossy().contains("projects"));
    }
}
