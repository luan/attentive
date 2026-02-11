//! File predictor with dual-mode prediction

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

static FILE_MENTION_RE: OnceLock<Regex> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Predictor {
    file_popularity: HashMap<String, usize>,
    co_occurrence: HashMap<String, HashMap<String, usize>>,
    name_to_paths: HashMap<String, Vec<String>>,
    strong_keywords: HashMap<String, String>,
    last_active_files: Vec<String>,
}

impl Predictor {
    pub fn new() -> Self {
        Self {
            file_popularity: HashMap::new(),
            co_occurrence: HashMap::new(),
            name_to_paths: HashMap::new(),
            strong_keywords: HashMap::new(),
            last_active_files: Vec::new(),
        }
    }

    pub fn train(&mut self, active_files_per_turn: &[Vec<String>]) {
        for files in active_files_per_turn {
            for file in files {
                *self.file_popularity.entry(file.clone()).or_insert(0) += 1;

                let basename = std::path::Path::new(file)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !basename.is_empty() {
                    self.name_to_paths
                        .entry(basename)
                        .or_default()
                        .push(file.clone());
                }
            }
            // Co-occurrence: every pair of files in same turn
            for (i, a) in files.iter().enumerate() {
                for b in files.iter().skip(i + 1) {
                    *self
                        .co_occurrence
                        .entry(a.clone())
                        .or_default()
                        .entry(b.clone())
                        .or_insert(0) += 1;
                    *self
                        .co_occurrence
                        .entry(b.clone())
                        .or_default()
                        .entry(a.clone())
                        .or_insert(0) += 1;
                }
            }
        }
    }

    pub fn predict(
        &self,
        prompt: &str,
        active_files: &[String],
        top_k: usize,
    ) -> Vec<(String, f64)> {
        let mut scores: HashMap<String, f64> = HashMap::new();

        // Confident mode: file mentions
        let mentions = extract_file_mentions(prompt);
        if !mentions.is_empty() {
            for mention in &mentions {
                // Direct path match
                if self.file_popularity.contains_key(mention) {
                    *scores.entry(mention.clone()).or_insert(0.0) += 1.0;
                }
                // Basename match
                let basename = mention.to_lowercase();
                if let Some(paths) = self.name_to_paths.get(&basename) {
                    for path in paths {
                        *scores.entry(path.clone()).or_insert(0.0) += 0.8;
                    }
                }
            }
        }

        // Confident mode: strong keywords
        let prompt_lower = prompt.to_lowercase();
        for (keyword, file_path) in &self.strong_keywords {
            if prompt_lower.contains(keyword) {
                *scores.entry(file_path.clone()).or_insert(0.0) += 0.9;
            }
        }

        // Co-occurrence boost from active files
        for active in active_files {
            if let Some(co_files) = self.co_occurrence.get(active) {
                for (co_file, &count) in co_files {
                    if !active_files.contains(co_file) {
                        let boost = (count as f64).min(5.0) / 5.0 * 0.6;
                        *scores.entry(co_file.clone()).or_insert(0.0) += boost;
                    }
                }
            }
        }

        // Fallback mode: popularity when no confident signals
        if scores.is_empty() {
            let max_pop = self.file_popularity.values().max().copied().unwrap_or(1) as f64;
            for (file, &count) in &self.file_popularity {
                if !active_files.contains(file) {
                    scores.insert(file.clone(), count as f64 / max_pop * 0.3);
                }
            }
        }

        let mut results: Vec<_> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }

    pub fn record_active(&mut self, files: &[String]) {
        self.last_active_files = files.to_vec();
    }
}

impl Default for Predictor {
    fn default() -> Self {
        Self::new()
    }
}

pub fn extract_file_mentions(prompt: &str) -> Vec<String> {
    let re = FILE_MENTION_RE.get_or_init(|| {
        Regex::new(r"\b([\w./-]+\.(?:rs|py|js|ts|tsx|jsx|go|java|md|json|html|css|yaml|yml|toml|c|cpp|h))\b").unwrap()
    });
    re.find_iter(prompt)
        .map(|m: regex::Match| m.as_str().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_file_mentions() {
        let mentions = extract_file_mentions("look at router.rs and config.json please");
        assert!(mentions.contains(&"router.rs".to_string()));
        assert!(mentions.contains(&"config.json".to_string()));
    }

    #[test]
    fn test_predict_file_mention_in_prompt() {
        let mut predictor = Predictor::new();
        predictor.train(&[vec!["router.rs".to_string()]]);
        let results = predictor.predict("fix router.rs", &[], 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "router.rs");
    }

    #[test]
    fn test_predict_fallback_popularity() {
        let mut predictor = Predictor::new();
        predictor.train(&[
            vec!["popular.rs".to_string()],
            vec!["popular.rs".to_string()],
            vec!["rare.rs".to_string()],
        ]);
        let results = predictor.predict("something unrelated", &[], 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "popular.rs");
    }

    #[test]
    fn test_predict_empty_predictor() {
        let predictor = Predictor::new();
        let results = predictor.predict("anything", &[], 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_json_roundtrip() {
        let mut predictor = Predictor::new();
        predictor.train(&[vec!["a.rs".to_string(), "b.rs".to_string()]]);
        let json = serde_json::to_string(&predictor).unwrap();
        let loaded: Predictor = serde_json::from_str(&json).unwrap();
        assert_eq!(
            loaded.file_popularity.len(),
            predictor.file_popularity.len()
        );
    }
}
