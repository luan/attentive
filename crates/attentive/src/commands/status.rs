use attentive_telemetry::Paths;

pub fn run() -> anyhow::Result<()> {
    let paths = Paths::new()?;

    println!("Attentive Status");
    println!("================");
    println!();

    // Check state files
    let files = vec![
        ("settings.json", paths.home_claude.join("settings.json")),
        ("attentive.json", paths.home_claude.join("attentive.json")),
        ("attn_state.json", paths.home_claude.join("attn_state.json")),
        (
            "learned_state.json",
            paths.home_claude.join("learned_state.json"),
        ),
        ("turns.jsonl", paths.turns_file()),
    ];

    for (name, path) in files {
        let status = if path.exists() { "✓" } else { "✗" };
        println!("  {} {}", status, name);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_runs() {
        // Just verify it doesn't crash
        let result = run();
        if let Err(e) = &result {
            eprintln!("Status error: {:?}", e);
        }
        assert!(result.is_ok(), "Status command failed");
    }
}
