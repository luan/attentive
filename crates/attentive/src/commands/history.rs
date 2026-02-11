use attentive_telemetry::{read_jsonl, Paths, TurnRecord};

#[derive(Default)]
struct HistoryFilter {
    file: Option<String>,
    hours: Option<u64>,
    limit: Option<usize>,
}

fn filter_turns<'a>(turns: &'a [TurnRecord], filter: &HistoryFilter) -> Vec<&'a TurnRecord> {
    let cutoff = filter
        .hours
        .map(|h| chrono::Utc::now() - chrono::Duration::hours(h as i64));

    turns
        .iter()
        .filter(|t| {
            if let Some(ref cutoff) = cutoff {
                if t.timestamp < *cutoff {
                    return false;
                }
            }
            if let Some(ref file) = filter.file {
                if !t.files_injected.contains(file) && !t.files_used.contains(file) {
                    return false;
                }
            }
            true
        })
        .collect()
}

fn compute_stats(turns: &[TurnRecord]) -> String {
    if turns.is_empty() {
        return "No turns to analyze.".to_string();
    }
    let total = turns.len();
    let avg_waste = turns.iter().map(|t| t.waste_ratio).sum::<f64>() / total as f64;
    let total_injected: usize = turns.iter().map(|t| t.injected_tokens).sum();
    let total_used: usize = turns.iter().map(|t| t.used_tokens).sum();

    format!(
        "Total turns: {}\n\
         Avg waste: {:.1}%\n\
         Total injected: {} tokens\n\
         Total used: {} tokens",
        total,
        avg_waste * 100.0,
        total_injected,
        total_used
    )
}

pub fn run(stats: bool) -> anyhow::Result<()> {
    let paths = Paths::new()?;
    let turns: Vec<TurnRecord> = read_jsonl(&paths.turns_file())?;

    if turns.is_empty() {
        println!("No turn history");
        return Ok(());
    }

    if stats {
        println!("{}", compute_stats(&turns));
        return Ok(());
    }

    let filter = HistoryFilter {
        limit: Some(20),
        ..Default::default()
    };

    let filtered = filter_turns(&turns, &filter);
    let display_turns: Vec<_> = filtered
        .into_iter()
        .rev()
        .take(filter.limit.unwrap_or(20))
        .collect();

    println!("Recent Turns (last {})", display_turns.len());
    println!("======================");
    for turn in &display_turns {
        println!(
            "  {} | injected:{} used:{} waste:{:.0}% conf:{:.0}%",
            turn.timestamp.format("%Y-%m-%d %H:%M"),
            turn.injected_tokens,
            turn.used_tokens,
            turn.waste_ratio * 100.0,
            turn.context_confidence.unwrap_or(0.0) * 100.0,
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_turns() -> Vec<TurnRecord> {
        vec![
            TurnRecord {
                turn_id: "t1".to_string(),
                session_id: "s1".to_string(),
                project: "/test".to_string(),
                timestamp: Utc::now() - chrono::Duration::hours(2),
                injected_tokens: 1000,
                used_tokens: 600,
                waste_ratio: 0.4,
                files_injected: vec!["a.rs".to_string()],
                files_used: vec!["a.rs".to_string()],
                was_notification: false,
                injection_chars: 4000,
                context_confidence: Some(0.8),
            },
            TurnRecord {
                turn_id: "t2".to_string(),
                session_id: "s1".to_string(),
                project: "/test".to_string(),
                timestamp: Utc::now(),
                injected_tokens: 2000,
                used_tokens: 1800,
                waste_ratio: 0.1,
                files_injected: vec!["b.rs".to_string()],
                files_used: vec!["b.rs".to_string()],
                was_notification: false,
                injection_chars: 8000,
                context_confidence: Some(0.95),
            },
        ]
    }

    #[test]
    fn test_filter_by_file() {
        let turns = sample_turns();
        let filtered = filter_turns(
            &turns,
            &HistoryFilter {
                file: Some("a.rs".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].turn_id, "t1");
    }

    #[test]
    fn test_stats_mode() {
        let turns = sample_turns();
        let stats = compute_stats(&turns);
        assert!(stats.contains("turns"));
        assert!(stats.contains("waste"));
    }

    #[test]
    fn test_no_filter_returns_all() {
        let turns = sample_turns();
        let filtered = filter_turns(&turns, &HistoryFilter::default());
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_history_stats_output() {
        let temp = tempfile::TempDir::new().unwrap();
        let turns_path = temp.path().join("turns.jsonl");
        let turn = attentive_telemetry::TurnRecord {
            turn_id: "t1".to_string(),
            session_id: "s1".to_string(),
            project: "/test".to_string(),
            timestamp: chrono::Utc::now(),
            injected_tokens: 1000,
            used_tokens: 400,
            waste_ratio: 0.6,
            files_injected: vec!["a.rs".to_string()],
            files_used: vec!["a.rs".to_string()],
            was_notification: false,
            injection_chars: 4000,
            context_confidence: Some(0.5),
        };
        let json = serde_json::to_string(&turn).unwrap();
        std::fs::write(&turns_path, format!("{}\n", json)).unwrap();

        let turns: Vec<attentive_telemetry::TurnRecord> =
            attentive_telemetry::read_jsonl(&turns_path).unwrap_or_default();
        let stats = compute_stats(&turns);
        assert!(stats.contains("Total turns"));
        assert!(stats.contains("Avg waste"));
    }
}
