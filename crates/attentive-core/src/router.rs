//! 7-phase attention router

use crate::config::Config;
use crate::types::{AttentionState, Tier};
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::Bfs;
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct Router {
    config: Config,
    co_activation_graph: Option<Graph<String, ()>>,
    node_indices: HashMap<String, NodeIndex>,
}

impl Router {
    pub fn new(config: Config) -> Self {
        let (graph, indices) = build_co_activation_graph(&config.co_activation);

        Self {
            config,
            co_activation_graph: Some(graph),
            node_indices: indices,
        }
    }

    /// Update attention scores based on prompt (7-phase with optional learner integration)
    pub fn update_attention(
        &self,
        state: &mut AttentionState,
        prompt: &str,
        learner: Option<&attentive_learn::Learner>,
    ) -> HashSet<String> {
        let directly_activated = HashSet::new();

        // Ensure consecutive_turns exists
        for path in state.scores.keys() {
            state.consecutive_turns.entry(path.clone()).or_insert(0);
        }

        // Phase 1: Decay with learned rates
        for (path, score) in &mut state.scores {
            let decay = if let Some(l) = learner {
                l.get_file_decay(path)
            } else {
                self.config.decay_rates.get_decay(path)
            };
            *score *= decay;
        }

        // Phase 2: Co-activation (direct neighbors + 2-hop transitive via BFS)
        if let Some(graph) = &self.co_activation_graph {
            let mut boosts: HashMap<String, f64> = HashMap::new();

            for activated_path in &directly_activated {
                if let Some(&node_idx) = self.node_indices.get(activated_path) {
                    // BFS to find neighbors up to 2 hops
                    let mut bfs = Bfs::new(graph, node_idx);
                    let mut visited = HashSet::new();
                    let mut hop_count = HashMap::new();
                    hop_count.insert(node_idx, 0);

                    while let Some(current_idx) = bfs.next(graph) {
                        if visited.contains(&current_idx) {
                            continue;
                        }
                        visited.insert(current_idx);

                        let current_hop = hop_count.get(&current_idx).copied().unwrap_or(0);
                        if current_hop > 2 {
                            continue;
                        }

                        // Get path for this node
                        if let Some(neighbor_path) = graph.node_weight(current_idx) {
                            if current_idx != node_idx {
                                // Direct neighbor (1-hop) or transitive (2-hop)
                                let boost = if current_hop == 1 {
                                    self.config.coactivation_boost // 0.35
                                } else {
                                    self.config.transitive_boost // 0.15
                                };

                                boosts
                                    .entry(neighbor_path.clone())
                                    .and_modify(|b| *b = b.max(boost))
                                    .or_insert(boost);
                            }

                            // Track hop count for neighbors
                            for neighbor_idx in graph.neighbors(current_idx) {
                                hop_count.entry(neighbor_idx).or_insert(current_hop + 1);
                            }
                        }
                    }
                }
            }

            // Apply boosts
            for (path, boost) in boosts {
                if let Some(score) = state.scores.get_mut(&path) {
                    *score = (*score + boost).min(1.0);
                }
            }
        }

        // Phase 3: Pinned file floor
        for pinned_path in &self.config.pinned_files {
            if let Some(score) = state.scores.get_mut(pinned_path) {
                let floor = self.config.warm_threshold + self.config.pinned_floor_boost;
                *score = score.max(floor);
            }
        }

        // Phase 4: Demoted file penalty
        for demoted_path in &self.config.demoted_files {
            if directly_activated.contains(demoted_path) {
                continue;
            }
            if let Some(score) = state.scores.get_mut(demoted_path) {
                *score *= self.config.demoted_penalty;
            }
        }

        // Phase 5: Learner boost (learned prompt-file associations)
        if let Some(l) = learner {
            let boosts = l.boost_scores(prompt, &state.scores);
            for (path, boosted_score) in boosts {
                if let Some(score) = state.scores.get_mut(&path) {
                    *score = boosted_score;
                }
            }
        }

        // Phase 6: Update consecutive_turns for cache stability
        for (path, &score) in &state.scores {
            let tier = Tier::from_score(score);
            if matches!(tier, Tier::Hot | Tier::Warm) {
                *state.consecutive_turns.entry(path.clone()).or_insert(0) += 1;
            } else {
                state.consecutive_turns.insert(path.clone(), 0);
            }
        }

        state.turn_count += 1;
        directly_activated
    }

    /// Build context output with cache stability sort
    pub fn build_context_output(
        &self,
        state: &AttentionState,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut hot_files = Vec::new();
        let mut warm_files = Vec::new();
        let mut cold_files = Vec::new();

        // Collect files by tier
        for (path, &score) in &state.scores {
            let tier = Tier::from_score(score);
            match tier {
                Tier::Hot => hot_files.push((path.clone(), score)),
                Tier::Warm => warm_files.push((path.clone(), score)),
                Tier::Cold => cold_files.push((path.clone(), score)),
            }
        }

        // Cache stability sort: pinned first, then by streak, then by score
        let sort_fn = |a: &(String, f64), b: &(String, f64)| {
            let a_pinned = self.config.pinned_files.contains(&a.0);
            let b_pinned = self.config.pinned_files.contains(&b.0);
            let a_streak = state.consecutive_turns.get(&a.0).copied().unwrap_or(0);
            let b_streak = state.consecutive_turns.get(&b.0).copied().unwrap_or(0);

            // Pinned first
            if a_pinned != b_pinned {
                return b_pinned.cmp(&a_pinned);
            }
            // Then by streak (descending)
            if a_streak != b_streak {
                return b_streak.cmp(&a_streak);
            }
            // Then by score (descending)
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        };

        hot_files.sort_by(sort_fn);
        warm_files.sort_by(sort_fn);

        // Apply limits
        hot_files.truncate(self.config.max_hot_files);
        warm_files.truncate(self.config.max_warm_files);

        (
            hot_files.into_iter().map(|(p, _)| p).collect(),
            warm_files.into_iter().map(|(p, _)| p).collect(),
            cold_files.into_iter().map(|(p, _)| p).collect(),
        )
    }
}

fn build_co_activation_graph(
    co_activation: &HashMap<String, Vec<String>>,
) -> (Graph<String, ()>, HashMap<String, NodeIndex>) {
    let mut graph = Graph::new();
    let mut node_indices = HashMap::new();

    // Add all nodes
    let mut all_nodes = HashSet::new();
    for (from, to_list) in co_activation {
        all_nodes.insert(from.clone());
        all_nodes.extend(to_list.iter().cloned());
    }

    for node in all_nodes {
        let idx = graph.add_node(node.clone());
        node_indices.insert(node, idx);
    }

    // Add edges
    for (from, to_list) in co_activation {
        if let Some(&from_idx) = node_indices.get(from) {
            for to in to_list {
                if let Some(&to_idx) = node_indices.get(to) {
                    graph.add_edge(from_idx, to_idx, ());
                }
            }
        }
    }

    (graph, node_indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decay_phase() {
        let config = Config::new();
        let router = Router::new(config);
        let mut state = AttentionState::new();
        state.scores.insert("file1.md".to_string(), 1.0);

        router.update_attention(&mut state, "other prompt", None);

        // Should have decayed (default 0.7)
        assert!(*state.scores.get("file1.md").unwrap() < 1.0);
        assert!(*state.scores.get("file1.md").unwrap() > 0.6);
    }

    #[test]
    fn test_build_context_output() {
        let config = Config::new();
        let router = Router::new(config);

        let mut state = AttentionState::new();
        state.scores.insert("hot1.md".to_string(), 0.9);
        state.scores.insert("warm1.md".to_string(), 0.5);
        state.scores.insert("cold1.md".to_string(), 0.1);

        let (hot, warm, cold) = router.build_context_output(&state);

        assert_eq!(hot, vec!["hot1.md"]);
        assert_eq!(warm, vec!["warm1.md"]);
        assert_eq!(cold, vec!["cold1.md"]);
    }

    #[test]
    fn test_demoted_file_penalty() {
        let mut config = Config::new();
        config.demoted_files.push("demoted.md".to_string());
        config.demoted_penalty = 0.5;

        let router = Router::new(config);
        let mut state = AttentionState::new();
        state.scores.insert("demoted.md".to_string(), 0.6);
        state.scores.insert("normal.md".to_string(), 0.6);

        router.update_attention(&mut state, "unrelated", None);

        // demoted.md: 0.6 * 0.7 (decay) * 0.5 (penalty) = 0.21
        let demoted_score = *state.scores.get("demoted.md").unwrap();
        assert!(
            demoted_score < 0.25,
            "Demoted file should be penalized: {}",
            demoted_score
        );

        // normal.md: 0.6 * 0.7 (decay) = 0.42, no penalty
        let normal_score = *state.scores.get("normal.md").unwrap();
        assert!(
            normal_score > 0.4,
            "Normal file should not be penalized: {}",
            normal_score
        );
    }

    #[test]
    fn test_learner_boost_applied() {
        // Create a learner in active mode with trained data
        let learner_json = r#"{"turn_count":30,"maturity":"active","word_file_counts":{"router":{"file1.md":10}},"word_doc_freq":{"router":15},"file_turns":{},"file_last_seen":{},"file_gaps":{},"last_session_files":[]}"#;
        let learner: attentive_learn::Learner = serde_json::from_str(learner_json).unwrap();

        let config = Config::new();
        let router = Router::new(config);
        let mut state = AttentionState::new();
        state.scores.insert("file1.md".to_string(), 0.3);

        router.update_attention(&mut state, "router", Some(&learner));

        // Score should be boosted above what pure decay would give (0.3 * 0.7 = 0.21)
        let score = *state.scores.get("file1.md").unwrap();
        assert!(
            score > 0.21,
            "Learner should boost score above decay: {}",
            score
        );
    }

    #[test]
    fn test_learned_decay_applied() {
        // Create a learner with custom decay for a file
        let mut learner = attentive_learn::Learner::new();
        // Simulate frequent access pattern (slow decay = 0.88)
        for i in 0..10 {
            if i % 2 == 0 {
                learner.observe_turn("frequent", &["freq.md".to_string()]);
            } else {
                learner.observe_turn("other", &["other.md".to_string()]);
            }
        }

        let config = Config::new();
        let router = Router::new(config);
        let mut state = AttentionState::new();
        state.scores.insert("freq.md".to_string(), 1.0);

        router.update_attention(&mut state, "unrelated", Some(&learner));

        // Should use learned slow decay (~0.88) instead of default (0.7)
        let score = *state.scores.get("freq.md").unwrap();
        assert!(
            score > 0.8,
            "Frequently accessed file should have slow decay: {}",
            score
        );
        assert!(score < 0.9, "Decay should still apply: {}", score);
    }
}
