use attentive_learn::Learner;

pub fn run() -> anyhow::Result<()> {
    let paths = attentive_telemetry::Paths::new()?;
    let state_path = paths.home_claude.join("learned_state.json");

    if !state_path.exists() {
        println!("No learned state found. Run attentive ingest first.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&state_path)?;
    let learner: Learner = serde_json::from_str(&content)?;
    let coactivation = learner.get_learned_coactivation();

    println!("Co-activation Graph");
    println!("===================");

    if coactivation.is_empty() {
        println!("No co-activation patterns detected yet.");
        return Ok(());
    }

    let mut pairs_shown = std::collections::HashSet::new();
    for (file, related) in &coactivation {
        for rel in related {
            let pair = if file < rel {
                (file.clone(), rel.clone())
            } else {
                (rel.clone(), file.clone())
            };
            if pairs_shown.insert(pair.clone()) {
                println!("  {} <-> {}", pair.0, pair.1);
            }
        }
    }

    println!("\n{} co-activation pairs found", pairs_shown.len());
    Ok(())
}
