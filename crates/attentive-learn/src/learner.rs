//! Learner for prompt-file affinity and co-activation patterns

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MATURITY_THRESHOLD: usize = 25;
const ACTIVE_BOOST_WEIGHT: f64 = 0.35;
const COACTIVATION_JACCARD_THRESHOLD: f64 = 0.25;
const DEFAULT_DECAY: f64 = 0.70;

static STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
    "do", "does", "did", "will", "would", "could", "should", "may", "might", "can", "to", "of",
    "in", "for", "on", "with", "at", "by", "from", "as", "into", "through", "then", "here",
    "there", "when", "where", "why", "how", "all", "each", "every", "both", "few", "more", "most",
    "some", "such", "not", "only", "just", "but", "and", "or", "if", "about", "what", "which",
    "who", "this", "that", "these", "those", "it", "its", "my", "me", "we", "our", "you", "your",
    "up", "down", "no", "so", "very", "too", "than", "please", "help", "want", "like", "think",
    "know", "see", "look", "make", "take", "get", "let", "say", "tell", "give", "use", "find",
    "show", "try", "ask", "work", "call", "put", "keep", "also", "file", "code", "change",
    "update", "add", "remove", "fix", "check", "run", "new", "now", "still", "already", "done",
    "good", "right", "sure", "yeah", "yes", "okay", "thanks", "thank",
];

/// Maturity level of the learner
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MaturityLevel {
    Observing, // 0-24 turns, no boost
    Active,    // 25+ turns, 0.35 boost
}

/// Learner state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Learner {
    turn_count: usize,
    maturity: MaturityLevel,
    // prompt word -> file -> co-occurrence count
    #[serde(default)]
    word_file_counts: HashMap<String, HashMap<String, usize>>,
    // word -> total document frequency (how many turns it appeared in)
    #[serde(default)]
    word_doc_freq: HashMap<String, usize>,
    // file -> set of turn indices where it was active
    #[serde(default)]
    file_turns: HashMap<String, HashSet<usize>>,
    // per-file access timestamps for rhythm detection
    #[serde(default)]
    file_last_seen: HashMap<String, usize>,
    #[serde(default)]
    file_gaps: HashMap<String, Vec<usize>>,
    // last session state for warm-start
    #[serde(default)]
    last_session_files: Vec<String>,
}

impl Learner {
    pub fn new() -> Self {
        Self {
            turn_count: 0,
            maturity: MaturityLevel::Observing,
            word_file_counts: HashMap::new(),
            word_doc_freq: HashMap::new(),
            file_turns: HashMap::new(),
            file_last_seen: HashMap::new(),
            file_gaps: HashMap::new(),
            last_session_files: Vec::new(),
        }
    }

    pub fn maturity(&self) -> MaturityLevel {
        self.maturity
    }

    pub fn boost_weight(&self) -> f64 {
        match self.maturity {
            MaturityLevel::Observing => 0.0,
            MaturityLevel::Active => ACTIVE_BOOST_WEIGHT,
        }
    }

    pub fn update_maturity(&mut self) {
        self.maturity = if self.turn_count >= MATURITY_THRESHOLD {
            MaturityLevel::Active
        } else {
            MaturityLevel::Observing
        };
    }

    /// Extract significant words from a prompt, filtering stop words
    fn extract_words(prompt: &str) -> Vec<String> {
        let stop_set: HashSet<&str> = STOP_WORDS.iter().copied().collect();
        prompt
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|w| w.len() >= 3 && !stop_set.contains(w))
            .map(|w| w.to_string())
            .collect()
    }

    /// Observe a turn: record prompt words and active files
    pub fn observe_turn(&mut self, prompt: &str, active_files: &[String]) {
        let words = Self::extract_words(prompt);
        if words.is_empty() || active_files.is_empty() {
            return;
        }

        // Track unique words in this turn for document frequency
        let unique_words: HashSet<String> = words.iter().cloned().collect();
        for word in &unique_words {
            *self.word_doc_freq.entry(word.clone()).or_insert(0) += 1;
        }

        // Track word-file co-occurrences
        for word in &words {
            let file_counts = self.word_file_counts.entry(word.clone()).or_default();
            for file in active_files {
                *file_counts.entry(file.clone()).or_insert(0) += 1;
            }
        }

        // Track file turns and gaps for rhythm detection
        for file in active_files {
            let turns = self.file_turns.entry(file.clone()).or_default();
            turns.insert(self.turn_count);

            // Update gaps
            if let Some(&last_seen) = self.file_last_seen.get(file) {
                let gap = self.turn_count.saturating_sub(last_seen);
                self.file_gaps.entry(file.clone()).or_default().push(gap);
            }
            self.file_last_seen.insert(file.clone(), self.turn_count);
        }

        self.turn_count += 1;
        self.update_maturity();
    }

    /// Calculate IDF for a word
    fn calculate_idf(&self, word: &str) -> f64 {
        if self.turn_count == 0 {
            return 1.0;
        }
        let doc_freq = self.word_doc_freq.get(word).copied().unwrap_or(0);
        let idf = (self.turn_count as f64 / (1.0 + doc_freq as f64)).ln();
        idf.max(0.1) // Clamp to minimum to avoid negative IDF for very common words
    }

    /// Boost scores based on learned associations
    pub fn boost_scores(
        &self,
        prompt: &str,
        current_scores: &HashMap<String, f64>,
    ) -> HashMap<String, f64> {
        if self.boost_weight() == 0.0 {
            return current_scores.clone();
        }

        let words = Self::extract_words(prompt);

        // If no valid words after filtering stop words, return scores unchanged
        if words.is_empty() {
            return current_scores.clone();
        }

        let mut boosted = current_scores.clone();

        // Calculate total words for normalization
        let total_words = words.len() as f64;

        // For each file in current scores, calculate learned boost
        for (file, base_score) in current_scores {
            let mut affinity_sum = 0.0;

            for word in &words {
                let idf = self.calculate_idf(word);
                if let Some(file_counts) = self.word_file_counts.get(word) {
                    if let Some(&count) = file_counts.get(file) {
                        // Normalize count by turn_count to get frequency
                        let frequency = if self.turn_count > 0 {
                            count as f64 / self.turn_count as f64
                        } else {
                            0.0
                        };
                        affinity_sum += idf * frequency;
                    }
                }
            }

            // Normalize by word count and apply maturity weight
            let normalized_affinity = affinity_sum / total_words.max(1.0);
            let boost = normalized_affinity * self.boost_weight();

            // Add boost, capped at 1.0
            boosted.insert(file.clone(), (base_score + boost).min(1.0));
        }

        boosted
    }

    /// Get learned co-activation patterns (files that appear together frequently)
    pub fn get_learned_coactivation(&self) -> HashMap<String, Vec<String>> {
        let mut coactivation: HashMap<String, Vec<String>> = HashMap::new();

        let files: Vec<&String> = self.file_turns.keys().collect();

        for (i, file_a) in files.iter().enumerate() {
            for file_b in files.iter().skip(i + 1) {
                let turns_a = &self.file_turns[*file_a];
                let turns_b = &self.file_turns[*file_b];

                // Calculate Jaccard similarity
                let intersection: HashSet<_> = turns_a.intersection(turns_b).collect();
                let union: HashSet<_> = turns_a.union(turns_b).collect();

                if union.is_empty() {
                    continue;
                }

                let jaccard = intersection.len() as f64 / union.len() as f64;

                // Threshold: Jaccard >= 0.25 and at least 3 co-occurrences
                if jaccard >= COACTIVATION_JACCARD_THRESHOLD && intersection.len() >= 3 {
                    coactivation
                        .entry((*file_a).clone())
                        .or_default()
                        .push((*file_b).clone());
                    coactivation
                        .entry((*file_b).clone())
                        .or_default()
                        .push((*file_a).clone());
                }
            }
        }

        coactivation
    }

    /// Get learned decay rate for a file based on revisit patterns
    pub fn get_file_decay(&self, path: &str) -> f64 {
        if let Some(gaps) = self.file_gaps.get(path) {
            if gaps.len() < 2 {
                return DEFAULT_DECAY;
            }

            // Calculate median gap
            let mut sorted_gaps = gaps.clone();
            sorted_gaps.sort_unstable();
            let median = sorted_gaps[sorted_gaps.len() / 2];

            // Map gap to decay rate:
            // Short gaps (frequently revisited) -> slow decay (0.88)
            // Long gaps (rarely revisited) -> fast decay (0.50)
            if median <= 3 {
                0.88
            } else if median >= 12 {
                0.50
            } else {
                // Linear interpolation between gap=3 and gap=12
                let t = (median as f64 - 3.0) / 9.0;
                0.88 + t * (0.50 - 0.88)
            }
        } else {
            DEFAULT_DECAY
        }
    }

    /// Get warm-up files from last session
    pub fn get_warmup(&self) -> Vec<String> {
        self.last_session_files.clone()
    }

    /// Save session state for warm-start
    pub fn save_session(&mut self, active_files: &[String]) {
        self.last_session_files = active_files.to_vec();
    }

    /// Get top N files by frequency (number of turns they appeared in)
    pub fn top_files_by_frequency(&self, limit: usize) -> Vec<(String, usize)> {
        let mut file_freq: Vec<(String, usize)> = self
            .file_turns
            .iter()
            .map(|(file, turns)| (file.clone(), turns.len()))
            .collect();

        file_freq.sort_by_key(|b| std::cmp::Reverse(b.1));
        file_freq.truncate(limit);
        file_freq
    }

    /// Count total unique word-file associations
    pub fn total_associations(&self) -> usize {
        self.word_file_counts
            .values()
            .map(|file_map| file_map.len())
            .sum()
    }
}

impl Default for Learner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observing_zero_boost() {
        let mut learner = Learner::new();
        for _ in 0..10 {
            learner.observe_turn("fix the router bug", &["router.rs".to_string()]);
        }
        let scores: HashMap<String, f64> = [("router.rs".to_string(), 0.5)].into();
        let boosts = learner.boost_scores("fix router", &scores);
        assert_eq!(*boosts.get("router.rs").unwrap_or(&0.0), 0.5);
    }

    #[test]
    fn test_active_mode_boosts() {
        let mut learner = Learner::new();
        for _ in 0..30 {
            learner.observe_turn("router config", &["router.rs".to_string()]);
        }
        let scores: HashMap<String, f64> = [("router.rs".to_string(), 0.5)].into();
        let boosts = learner.boost_scores("router", &scores);
        let boost = *boosts.get("router.rs").unwrap_or(&0.0);
        assert!(
            boost > 0.5,
            "Active learner should boost beyond base score: {}",
            boost
        );
        assert!(
            boost <= 1.0,
            "Boost should not exceed 1.0 (capped): {}",
            boost
        );
    }

    #[test]
    fn test_idf_dampens_common_words() {
        let mut learner = Learner::new();
        for _ in 0..30 {
            learner.observe_turn("the router is broken", &["router.rs".to_string()]);
            learner.observe_turn("the config is wrong", &["config.rs".to_string()]);
        }
        // "the" appears in every turn -> low IDF, "router" only in half -> higher IDF
        let scores: HashMap<String, f64> = [
            ("router.rs".to_string(), 0.5),
            ("config.rs".to_string(), 0.5),
        ]
        .into();
        let boosts_router = learner.boost_scores("router", &scores);
        let boosts_the = learner.boost_scores("the", &scores);
        let router_boost = *boosts_router.get("router.rs").unwrap_or(&0.0);
        let the_boost_router = *boosts_the.get("router.rs").unwrap_or(&0.0);

        // "the" is a stop word — filtered by extract_words, so boost_scores("the")
        // returns base score unchanged (0.5). "router" has learned affinity, so
        // boost_scores("router") should return > 0.5 for router.rs.
        assert!(
            router_boost > the_boost_router,
            "Domain term should boost router.rs more than stop word: router={}, the={}",
            router_boost,
            the_boost_router
        );
    }

    #[test]
    fn test_coactivation_detection() {
        let mut learner = Learner::new();
        // files a and b always appear together
        for _ in 0..5 {
            learner.observe_turn("test", &["a.rs".to_string(), "b.rs".to_string()]);
        }
        // file c appears alone
        for _ in 0..5 {
            learner.observe_turn("other", &["c.rs".to_string()]);
        }
        let coact = learner.get_learned_coactivation();
        assert!(
            coact
                .get("a.rs")
                .map_or(false, |v| v.contains(&"b.rs".to_string())),
            "a.rs and b.rs should co-activate"
        );
        assert!(
            !coact
                .get("a.rs")
                .map_or(false, |v| v.contains(&"c.rs".to_string())),
            "a.rs and c.rs should not co-activate"
        );
    }

    #[test]
    fn test_json_roundtrip() {
        let mut learner = Learner::new();
        for _ in 0..5 {
            learner.observe_turn("test prompt", &["file.rs".to_string()]);
        }
        let json = serde_json::to_string(&learner).unwrap();
        let loaded: Learner = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.turn_count, learner.turn_count);
        assert_eq!(loaded.maturity, learner.maturity);
    }

    #[test]
    fn test_warm_start() {
        let mut learner = Learner::new();
        learner.save_session(&["a.rs".to_string(), "b.rs".to_string()]);
        let warmup = learner.get_warmup();
        assert_eq!(warmup, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn test_file_decay_slow_for_frequent() {
        let mut learner = Learner::new();
        // Simulate frequent revisits (gap = 2)
        for i in 0..10 {
            if i % 2 == 0 {
                learner.observe_turn("test", &["freq.rs".to_string()]);
            } else {
                learner.observe_turn("test", &["other.rs".to_string()]);
            }
        }
        let decay = learner.get_file_decay("freq.rs");
        assert!(
            decay > 0.8,
            "Frequently accessed files should have slow decay: {}",
            decay
        );
    }

    #[test]
    fn test_file_decay_fast_for_rare() {
        let mut learner = Learner::new();
        // Simulate rare revisits (large gaps)
        learner.observe_turn("test", &["rare.rs".to_string()]);
        for _ in 0..15 {
            learner.observe_turn("other", &["other.rs".to_string()]);
        }
        learner.observe_turn("test", &["rare.rs".to_string()]);
        for _ in 0..15 {
            learner.observe_turn("other", &["other.rs".to_string()]);
        }
        learner.observe_turn("test", &["rare.rs".to_string()]);

        let decay = learner.get_file_decay("rare.rs");
        assert!(
            decay < 0.7,
            "Rarely accessed files should have fast decay: {}",
            decay
        );
    }

    #[test]
    fn test_boost_scores_stopwords_only_returns_unchanged() {
        let mut learner = Learner::new();
        for _ in 0..30 {
            learner.observe_turn("router config", &["router.rs".to_string()]);
        }
        let scores: HashMap<String, f64> = [("router.rs".to_string(), 0.7)].into();
        let boosts = learner.boost_scores("the is are", &scores);
        assert_eq!(
            *boosts.get("router.rs").unwrap_or(&0.0),
            0.7,
            "Stop-word-only prompt should return scores unchanged"
        );
    }
}
