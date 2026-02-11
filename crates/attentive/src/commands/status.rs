use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};

use attentive_core::AttentionState;
use attentive_telemetry::Paths;

pub fn run(session: Option<&str>) -> anyhow::Result<()> {
    let paths = Paths::new()?;
    let state_path = paths.attn_state_path()?;

    let state: Option<AttentionState> = if state_path.exists() {
        std::fs::read_to_string(&state_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
    } else {
        None
    };

    let (hot, warm, cold, hot_files) = match &state {
        Some(s) => {
            let mut h = 0usize;
            let mut w = 0usize;
            let mut c = 0usize;
            let mut hot_set = HashSet::new();
            for (path, &score) in &s.scores {
                if score >= 0.8 {
                    h += 1;
                    hot_set.insert(path.clone());
                } else if score >= 0.25 {
                    w += 1;
                } else {
                    c += 1;
                }
            }
            (h, w, c, hot_set)
        }
        None => (0, 0, 0, HashSet::new()),
    };

    let effectiveness = session.and_then(|sid| {
        let project_dir = paths.project_dir().ok()?;
        let transcript = project_dir.join(format!("{sid}.jsonl"));
        compute_effectiveness(&transcript, &hot_files)
    });

    let mut output = serde_json::json!({
        "hot": hot,
        "warm": warm,
        "cold": cold,
    });

    if let Some(eff) = effectiveness {
        output["saved"] = serde_json::json!(eff.saved);
        output["redundant"] = serde_json::json!(eff.redundant);
        output["missed"] = serde_json::json!(eff.missed);
        output["hit_rate"] = serde_json::json!(eff.hit_rate());
    }

    println!("{output}");
    Ok(())
}

struct Effectiveness {
    saved: usize,
    redundant: usize,
    missed: usize,
}

impl Effectiveness {
    fn hit_rate(&self) -> i32 {
        let total = self.saved + self.redundant + self.missed;
        if total == 0 {
            return -1;
        }
        (self.saved as f64 / total as f64 * 100.0) as i32
    }
}

fn compute_effectiveness(
    transcript: &std::path::Path,
    hot_files: &HashSet<String>,
) -> Option<Effectiveness> {
    let file = std::fs::File::open(transcript).ok()?;
    let reader = BufReader::new(file);

    // Track per-file: which tools touched it
    let mut file_tools: HashMap<String, HashSet<String>> = HashMap::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let turn: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if turn.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let Some(content) = turn.pointer("/message/content").and_then(|c| c.as_array()) else {
            continue;
        };
        for item in content {
            if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                continue;
            }
            let tool = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let Some(input) = item.get("input") else {
                continue;
            };
            let target = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .or_else(|| input.get("notebook_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if target.starts_with('/') {
                file_tools
                    .entry(target.to_string())
                    .or_default()
                    .insert(tool.to_string());
            }
        }
    }

    let mut saved = 0;
    let mut redundant = 0;
    let mut missed = 0;

    for (file, tools) in &file_tools {
        let was_hot = hot_files.contains(file);
        let was_read = tools.contains("Read");
        let was_written = tools.contains("Edit") || tools.contains("Write");

        if was_hot && was_written && !was_read {
            // HOT file edited without needing a Read — injection saved a round trip
            saved += 1;
        } else if was_hot && was_read {
            // HOT file but Claude Read it anyway — injection was redundant
            redundant += 1;
        } else if !was_hot && was_read {
            // Not HOT but Claude needed it — attentive missed predicting this file
            missed += 1;
        }
        // Files only written (not hot, not read) are ignored — Claude knew what to write
    }

    Some(Effectiveness {
        saved,
        redundant,
        missed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effectiveness_empty_transcript() {
        let result = compute_effectiveness(std::path::Path::new("/nonexistent"), &HashSet::new());
        assert!(result.is_none());
    }

    #[test]
    fn test_effectiveness_categories() {
        let temp = tempfile::TempDir::new().unwrap();
        let transcript = temp.path().join("test.jsonl");

        // Turn 1: Read a.rs (missed — not HOT), Edit b.rs without Read (saved — HOT)
        // Turn 2: Read c.rs (redundant — HOT but Read anyway)
        let turns = [
            serde_json::json!({
                "type": "assistant",
                "message": { "content": [
                    {"type": "tool_use", "name": "Read", "input": {"file_path": "/src/a.rs"}},
                    {"type": "tool_use", "name": "Edit", "input": {"file_path": "/src/b.rs"}},
                ]}
            }),
            serde_json::json!({
                "type": "assistant",
                "message": { "content": [
                    {"type": "tool_use", "name": "Read", "input": {"file_path": "/src/c.rs"}},
                ]}
            }),
        ];
        let content: String = turns.iter().map(|t| format!("{t}\n")).collect();
        std::fs::write(&transcript, content).unwrap();

        let hot: HashSet<String> = ["/src/b.rs", "/src/c.rs"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        let eff = compute_effectiveness(&transcript, &hot).unwrap();
        assert_eq!(eff.saved, 1); // b.rs: HOT + Edit, no Read
        assert_eq!(eff.redundant, 1); // c.rs: HOT but Read anyway
        assert_eq!(eff.missed, 1); // a.rs: not HOT, had to Read
        assert_eq!(eff.hit_rate(), 33); // 1/3
    }
}
