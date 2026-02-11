pub fn run() -> anyhow::Result<()> {
    let paths = attentive_telemetry::Paths::new()?;
    let db_path = paths.home_claude.join("observations.db");

    if !db_path.exists() {
        println!("No observations database found. Run some sessions first.");
        return Ok(());
    }

    let db = attentive_compress::ObservationDb::new(&db_path)?;
    let index = db.get_index()?;

    println!("Compressed Observations: {}", index.len());
    println!("========================");

    if index.is_empty() {
        println!("No observations stored yet.");
        return Ok(());
    }

    // Show summary stats
    let total_tokens: i64 = index.iter().map(|e| e.token_count).sum();
    println!("Total compressed tokens: {}", total_tokens);
    println!();

    // Group by type
    let mut by_type: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for entry in &index {
        *by_type.entry(entry.obs_type.clone()).or_default() += 1;
    }
    println!("By type:");
    for (obs_type, count) in &by_type {
        println!("  {}: {}", obs_type, count);
    }

    println!("\nRecent (last 10):");
    for entry in index.iter().take(10) {
        println!(
            "  {} [{}] {} ({} tokens)",
            entry.date, entry.obs_type, entry.title, entry.token_count
        );
    }
    Ok(())
}
