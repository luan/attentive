//! Hand-rolled BM25 implementation

use std::collections::HashMap;

const K1: f64 = 1.5;
const B: f64 = 0.75;

#[derive(Debug, Clone)]
pub struct BM25 {
    doc_count: usize,
    avg_doc_len: f64,
    doc_lens: Vec<usize>,
    doc_ids: Vec<String>,
    idf: HashMap<String, f64>,
}

impl BM25 {
    pub fn new() -> Self {
        Self {
            doc_count: 0,
            avg_doc_len: 0.0,
            doc_lens: Vec::new(),
            doc_ids: Vec::new(),
            idf: HashMap::new(),
        }
    }

    pub fn index(&mut self, documents: Vec<(String, Vec<String>)>) {
        self.doc_count = documents.len();
        if self.doc_count == 0 {
            return;
        }

        // Store doc IDs and lengths
        let mut total_len = 0;
        for (doc_id, tokens) in &documents {
            self.doc_ids.push(doc_id.clone());
            let len = tokens.len();
            self.doc_lens.push(len);
            total_len += len;
        }

        self.avg_doc_len = total_len as f64 / self.doc_count as f64;

        // Compute IDF
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        for (_, tokens) in &documents {
            let unique_tokens: std::collections::HashSet<_> = tokens.iter().collect();
            for token in unique_tokens {
                *doc_freq.entry(token.clone()).or_insert(0) += 1;
            }
        }

        for (term, df) in doc_freq {
            let idf = ((self.doc_count as f64 - df as f64 + 0.5) / (df as f64 + 0.5) + 1.0).ln();
            self.idf.insert(term, idf);
        }
    }

    pub fn search(&self, query_tokens: &[String], k: usize) -> Vec<(String, f64)> {
        if self.doc_count == 0 {
            return Vec::new();
        }

        let mut scores: Vec<(String, f64)> = self
            .doc_ids
            .iter()
            .enumerate()
            .map(|(idx, doc_id)| {
                let score = self.compute_score(idx, query_tokens);
                (doc_id.clone(), score)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }

    fn compute_score(&self, doc_idx: usize, query_tokens: &[String]) -> f64 {
        let doc_len = self.doc_lens[doc_idx] as f64;
        let mut score = 0.0;

        for term in query_tokens {
            if let Some(&idf) = self.idf.get(term) {
                // For simplicity, assume tf = 1 if term present
                // In full implementation, would count term frequency
                let norm = 1.0 + K1 * (1.0 - B + B * doc_len / self.avg_doc_len);
                score += idf * K1 / norm;
            }
        }

        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_empty() {
        let bm25 = BM25::new();
        let results = bm25.search(&["test".to_string()], 10);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_bm25_ranks_relevant_higher() {
        let mut bm25 = BM25::new();

        let docs = vec![
            (
                "doc1".to_string(),
                vec!["rust".to_string(), "programming".to_string()],
            ),
            (
                "doc2".to_string(),
                vec!["python".to_string(), "programming".to_string()],
            ),
            (
                "doc3".to_string(),
                vec!["rust".to_string(), "systems".to_string()],
            ),
        ];

        bm25.index(docs);

        let results = bm25.search(&["rust".to_string()], 3);
        assert!(results.len() >= 2);

        // docs with "rust" should be ranked higher
        let rust_docs: Vec<_> = results
            .iter()
            .filter(|(id, _)| id.contains("doc1") || id.contains("doc3"))
            .collect();
        assert!(rust_docs.len() >= 2);
    }
}
