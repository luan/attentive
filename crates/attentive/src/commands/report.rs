use attentive_telemetry::{Paths, TurnRecord, read_jsonl};
use std::collections::HashMap;

pub fn run() -> anyhow::Result<()> {
    let paths = Paths::new()?;
    let turns: Vec<TurnRecord> = read_jsonl(&paths.turns_file())?;
    let report = build_report(&turns);
    println!("{}", report);
    Ok(())
}

fn build_report(turns: &[TurnRecord]) -> String {
    if turns.is_empty() {
        return "No turns recorded yet.".to_string();
    }

    let mut sections = Vec::new();

    // Section 1: Summary
    let total_injected: usize = turns.iter().map(|t| t.injected_tokens).sum();
    let total_used: usize = turns.iter().map(|t| t.used_tokens).sum();
    let avg_waste = if total_injected > 0 {
        1.0 - (total_used as f64 / total_injected as f64)
    } else {
        0.0
    };

    sections.push(format!(
        "Token Usage Report\n==================\n\
         Total turns: {}\nTotal injected: {}\nTotal used: {}\n\
         Average waste: {:.1}%",
        turns.len(),
        total_injected,
        total_used,
        avg_waste * 100.0
    ));

    // Section 2: Waste Analysis
    let waste_ratios: Vec<f64> = turns.iter().map(|t| t.waste_ratio).collect();
    let median_waste = {
        let mut sorted = waste_ratios.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted[sorted.len() / 2]
    };
    let notif_count = turns.iter().filter(|t| t.was_notification).count();
    sections.push(format!(
        "\nWaste Analysis\n--------------\n\
         Mean waste: {:.1}% | Median: {:.1}%\n\
         Notification turns: {}/{} ({:.0}%)",
        avg_waste * 100.0,
        median_waste * 100.0,
        notif_count,
        turns.len(),
        notif_count as f64 / turns.len() as f64 * 100.0
    ));

    // Section 3: Confidence
    let confidences: Vec<f64> = turns.iter().filter_map(|t| t.context_confidence).collect();
    if !confidences.is_empty() {
        let avg_conf = confidences.iter().sum::<f64>() / confidences.len() as f64;
        sections.push(format!(
            "\nConfidence\n----------\n\
             Average context confidence: {:.1}% ({} turns with data)",
            avg_conf * 100.0,
            confidences.len()
        ));
    }

    // Section 4: File Leaderboard
    let leaderboard = build_file_leaderboard(turns);
    if !leaderboard.is_empty() {
        sections.push(format!(
            "\nFile Leaderboard\n----------------\n{}",
            leaderboard
        ));
    }

    sections.join("\n")
}

fn build_file_leaderboard(turns: &[TurnRecord]) -> String {
    let mut injected_count: HashMap<&str, usize> = HashMap::new();
    let mut used_count: HashMap<&str, usize> = HashMap::new();

    for t in turns {
        for f in &t.files_injected {
            *injected_count.entry(f.as_str()).or_default() += 1;
        }
        for f in &t.files_used {
            *used_count.entry(f.as_str()).or_default() += 1;
        }
    }

    let mut files: Vec<_> = injected_count
        .iter()
        .map(|(&f, &inj)| {
            let used = used_count.get(f).copied().unwrap_or(0);
            let efficiency = if inj > 0 {
                used as f64 / inj as f64
            } else {
                0.0
            };
            (f, inj, used, efficiency)
        })
        .collect();

    files.sort_by_key(|x| std::cmp::Reverse(x.1));

    files
        .iter()
        .take(10)
        .map(|(f, inj, used, eff)| {
            let name = std::path::Path::new(f)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(f);
            format!(
                "  {} — injected:{} used:{} efficiency:{:.0}%",
                name,
                inj,
                used,
                eff * 100.0
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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
                timestamp: Utc::now(),
                injected_tokens: 1000,
                used_tokens: 600,
                waste_ratio: 0.4,
                files_injected: vec!["a.rs".to_string(), "b.rs".to_string()],
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
                files_injected: vec!["a.rs".to_string(), "c.rs".to_string()],
                files_used: vec!["a.rs".to_string(), "c.rs".to_string()],
                was_notification: false,
                injection_chars: 8000,
                context_confidence: Some(0.95),
            },
        ]
    }

    #[test]
    fn test_build_report_has_sections() {
        let turns = sample_turns();
        let report = build_report(&turns);
        assert!(report.contains("Token Usage Report"));
        assert!(report.contains("Waste Analysis"));
        assert!(report.contains("File Leaderboard"));
        assert!(report.contains("Confidence"));
    }

    #[test]
    fn test_build_report_empty() {
        let report = build_report(&[]);
        assert!(report.contains("No turns"));
    }

    #[test]
    fn test_file_leaderboard_sorted() {
        let turns = sample_turns();
        let leaderboard = build_file_leaderboard(&turns);
        // a.rs appears in both turns, should rank high
        assert!(leaderboard.contains("a.rs"));
    }
}
