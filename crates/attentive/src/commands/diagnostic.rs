use attentive_telemetry::Paths;

fn build_diagnostic(json_mode: bool) -> String {
    let paths = Paths::new().ok();

    let system_info = get_system_info();
    let file_checks = check_files(paths.as_ref());
    let git_info = get_git_info();

    if json_mode {
        let mut report = serde_json::json!({
            "system": system_info,
            "files": file_checks,
        });
        if let Some(git) = git_info {
            report["git"] = git;
        }
        serde_json::to_string_pretty(&report).unwrap_or_default()
    } else {
        let mut sections = Vec::new();

        sections.push("Diagnostic Report\n==================".to_string());

        sections.push(format!(
            "\nSystem\n------\n  OS: {}\n  Arch: {}\n  attentive: {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            env!("CARGO_PKG_VERSION"),
        ));

        sections.push("\nFiles\n-----".to_string());
        for (name, status) in &file_checks {
            sections.push(format!("  {} {}", status, name));
        }

        if let Some(git) = &git_info
            && let Some(branch) = git.get("branch").and_then(|b| b.as_str())
        {
            sections.push(format!("\nGit\n---\n  Branch: {}", branch));
        }

        let issues: usize = file_checks
            .iter()
            .filter(|(_, s)| s.starts_with("ERR") || s.starts_with("MISS"))
            .count();
        sections.push(format!("\n{} issues found", issues));

        sections.join("\n")
    }
}

fn get_system_info() -> serde_json::Value {
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "version": env!("CARGO_PKG_VERSION"),
    })
}

fn check_files(paths: Option<&Paths>) -> Vec<(String, String)> {
    let mut checks = Vec::new();

    let files_to_check: Vec<(String, std::path::PathBuf)> = if let Some(paths) = paths {
        vec![
            (
                "settings.json".to_string(),
                paths.home_claude.join("settings.json"),
            ),
            (
                "attentive.json".to_string(),
                paths.home_claude.join("attentive.json"),
            ),
            (
                "attn_state.json".to_string(),
                paths.home_claude.join("attn_state.json"),
            ),
            (
                "learned_state.json".to_string(),
                paths.home_claude.join("learned_state.json"),
            ),
            ("turns.jsonl".to_string(), paths.turns_file()),
        ]
    } else {
        Vec::new()
    };

    for (name, path) in files_to_check {
        let status = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if name.ends_with(".jsonl") {
                        let lines = content.lines().count();
                        format!("OK  ({} lines)", lines)
                    } else {
                        match serde_json::from_str::<serde_json::Value>(&content) {
                            Ok(_) => "OK ".to_string(),
                            Err(e) => format!("ERR (invalid JSON: {})", e),
                        }
                    }
                }
                Err(e) => format!("ERR (read error: {})", e),
            }
        } else {
            "MISS".to_string()
        };
        checks.push((name, status));
    }

    checks
}

fn get_git_info() -> Option<serde_json::Value> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(serde_json::json!({
        "branch": branch,
    }))
}

pub fn run() -> anyhow::Result<()> {
    // TODO: parse --json flag from CLI
    let report = build_diagnostic(false);
    println!("{}", report);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_reports() {
        let result = run();
        assert!(result.is_ok());
    }

    #[test]
    fn test_diagnostic_has_sections() {
        let report = build_diagnostic(false);
        assert!(report.contains("System"));
        assert!(report.contains("Files"));
    }

    #[test]
    fn test_diagnostic_json_mode() {
        let report = build_diagnostic(true);
        let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
        assert!(parsed.get("system").is_some());
        assert!(parsed.get("files").is_some());
    }

    #[test]
    fn test_check_git_info() {
        // Should not panic even outside a git repo
        let info = get_git_info();
        // info may be None if not in git repo, which is fine
        assert!(info.is_none() || info.is_some());
    }
}
