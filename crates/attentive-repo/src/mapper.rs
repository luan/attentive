//! Repository mapper with PageRank-based ranking

use crate::symbols::{FileSymbols, extract_symbols};
use petgraph::algo::page_rank;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;

/// Repository mapper for symbol extraction and ranking
pub struct RepoMapper {
    file_symbols: HashMap<String, FileSymbols>,
    dependency_graph: DiGraph<String, ()>,
    node_indices: HashMap<String, NodeIndex>,
}

impl RepoMapper {
    pub fn new() -> Self {
        Self {
            file_symbols: HashMap::new(),
            dependency_graph: DiGraph::new(),
            node_indices: HashMap::new(),
        }
    }

    /// Add a file's symbols to the mapper
    pub fn add_file(&mut self, path: &str, content: &str) {
        let symbols = match extract_symbols(content, path) {
            Some(s) => s,
            None => return,
        };

        // Add node to graph
        let idx = self.dependency_graph.add_node(path.to_string());
        self.node_indices.insert(path.to_string(), idx);

        // Add edges for imports
        for import in &symbols.imports {
            if import.is_empty() {
                continue;
            }
            // Try direct match first, then with language-specific extension
            let target_idx = if let Some(&tidx) = self.node_indices.get(import) {
                Some(tidx)
            } else {
                // Try common extensions based on language
                let extensions = match symbols.language.as_str() {
                    "python" => vec![".py"],
                    "javascript" => vec![".js", ".jsx", ".ts", ".tsx"],
                    "rust" => vec![".rs"],
                    "go" => vec![".go"],
                    "java" => vec![".java"],
                    "c" => vec![".c", ".cpp", ".cc", ".h", ".hpp"],
                    _ => vec![],
                };

                extensions.iter().find_map(|ext| {
                    let with_ext = format!("{}{}", import, ext);
                    self.node_indices.get(&with_ext).copied()
                })
            };

            if let Some(tidx) = target_idx {
                self.dependency_graph.add_edge(idx, tidx, ());
            }
        }

        self.file_symbols.insert(path.to_string(), symbols);
    }

    /// Get PageRank scores for all files
    pub fn page_rank(&self) -> HashMap<String, f64> {
        if self.dependency_graph.node_count() == 0 {
            return HashMap::new();
        }

        let scores = page_rank(&self.dependency_graph, 0.85, 100);

        self.node_indices
            .iter()
            .map(|(path, &idx)| (path.clone(), scores[idx.index()]))
            .collect()
    }

    /// Get symbols for a file
    pub fn get_symbols(&self, path: &str) -> Option<&FileSymbols> {
        self.file_symbols.get(path)
    }

    /// Get ranked files respecting token budget
    pub fn get_ranked_files(&self, token_budget: usize) -> Vec<String> {
        let mut ranks: Vec<_> = self.page_rank().into_iter().collect();
        ranks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut result = Vec::new();
        let mut tokens_used = 0;

        for (path, _score) in ranks {
            if let Some(symbols) = self.file_symbols.get(&path) {
                if tokens_used + symbols.token_estimate > token_budget {
                    break;
                }
                tokens_used += symbols.token_estimate;
                result.push(path);
            }
        }

        result
    }
}

impl Default for RepoMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapper_add_file() {
        let mut mapper = RepoMapper::new();
        let code = "def foo():\n    pass";
        mapper.add_file("test.py", code);

        let symbols = mapper.get_symbols("test.py").unwrap();
        assert_eq!(symbols.symbols.len(), 1);
        assert_eq!(symbols.symbols[0].name, "foo");
    }

    #[test]
    fn test_pagerank_higher_for_imported() {
        let mut mapper = RepoMapper::new();

        // lib.py imports utils.py
        mapper.add_file("utils.py", "def helper(): pass");
        mapper.add_file("lib.py", "from utils import helper\ndef foo(): pass");

        let ranks = mapper.page_rank();

        // utils.py should rank higher because it's imported
        assert!(ranks.get("utils.py").unwrap_or(&0.0) > ranks.get("lib.py").unwrap_or(&0.0));
    }

    #[test]
    fn test_token_budget_respected() {
        let mut mapper = RepoMapper::new();
        mapper.add_file("a.py", "def foo(): pass");
        mapper.add_file("b.py", "def bar(): pass");
        mapper.add_file("c.py", "def baz(): pass");

        let ranked = mapper.get_ranked_files(20); // Only 1-2 files fit
        assert!(ranked.len() <= 2);
    }
}
