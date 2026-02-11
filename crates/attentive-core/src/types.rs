//! Core types for attention routing

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Attention tier classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    /// HOT (>0.8): Full file injection
    #[serde(rename = "HOT")]
    Hot,
    /// WARM (0.25-0.8): Compressed TOC
    #[serde(rename = "WARM")]
    Warm,
    /// COLD (<0.25): Evicted
    #[serde(rename = "COLD")]
    Cold,
}

impl Tier {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            Tier::Hot
        } else if score >= 0.25 {
            Tier::Warm
        } else {
            Tier::Cold
        }
    }
}

/// Attention state (compatible with Python attn_state.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionState {
    /// Attention scores per file path
    pub scores: HashMap<String, f64>,
    /// Consecutive turns each file has been HOT/WARM
    pub consecutive_turns: HashMap<String, usize>,
    /// Total turn count
    #[serde(default)]
    pub turn_count: usize,
}

impl AttentionState {
    pub fn new() -> Self {
        Self {
            scores: HashMap::new(),
            consecutive_turns: HashMap::new(),
            turn_count: 0,
        }
    }

    pub fn get_tier(&self, path: &str) -> Option<Tier> {
        self.scores.get(path).map(|&score| Tier::from_score(score))
    }

    pub fn get_hot_files(&self) -> Vec<String> {
        self.scores
            .iter()
            .filter(|(_, &score)| score >= 0.8)
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub fn get_warm_files(&self) -> Vec<String> {
        self.scores
            .iter()
            .filter(|(_, &score)| (0.25..0.8).contains(&score))
            .map(|(path, _)| path.clone())
            .collect()
    }
}

impl Default for AttentionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_from_score() {
        assert_eq!(Tier::from_score(0.9), Tier::Hot);
        assert_eq!(Tier::from_score(0.5), Tier::Warm);
        assert_eq!(Tier::from_score(0.1), Tier::Cold);
    }

    #[test]
    fn test_state_roundtrip() {
        let mut state = AttentionState::new();
        state.scores.insert("file1.md".to_string(), 0.9);
        state.consecutive_turns.insert("file1.md".to_string(), 3);

        let json = serde_json::to_string(&state).unwrap();
        let parsed: AttentionState = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.scores.get("file1.md"), Some(&0.9));
        assert_eq!(parsed.consecutive_turns.get("file1.md"), Some(&3));
    }
}
