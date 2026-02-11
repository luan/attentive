//! SimpleTFIDF fallback implementation

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SimpleTFIDF {
    vocab: HashMap<String, usize>,
    idf: HashMap<String, f64>,
    doc_vecs: Vec<Vec<f64>>,
    doc_paths: Vec<String>,
}

impl SimpleTFIDF {
    pub fn new() -> Self {
        Self {
            vocab: HashMap::new(),
            idf: HashMap::new(),
            doc_vecs: Vec::new(),
            doc_paths: Vec::new(),
        }
    }

    pub fn index(&mut self, documents: Vec<(String, Vec<String>)>) {
        if documents.is_empty() {
            return;
        }

        self.doc_paths = documents.iter().map(|(p, _)| p.clone()).collect();

        // Build vocabulary
        let mut vocab_set = std::collections::HashSet::new();
        for (_, tokens) in &documents {
            vocab_set.extend(tokens.iter().cloned());
        }
        let mut vocab_vec: Vec<_> = vocab_set.into_iter().collect();
        vocab_vec.sort();
        self.vocab = vocab_vec
            .iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), i))
            .collect();

        // Compute IDF
        let doc_count = documents.len();
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        for (_, tokens) in &documents {
            let unique: std::collections::HashSet<_> = tokens.iter().collect();
            for token in unique {
                *doc_freq.entry(token.clone()).or_insert(0) += 1;
            }
        }

        for (term, df) in doc_freq {
            let idf = ((doc_count + 1) as f64 / (df + 1) as f64).ln() + 1.0;
            self.idf.insert(term, idf);
        }

        // Build TF-IDF vectors
        for (_, tokens) in &documents {
            let mut tf: HashMap<String, usize> = HashMap::new();
            for token in tokens {
                *tf.entry(token.clone()).or_insert(0) += 1;
            }

            let mut vec = vec![0.0; self.vocab.len()];
            for (term, count) in tf {
                if let Some(&idx) = self.vocab.get(&term) {
                    let idf_val = self.idf.get(&term).copied().unwrap_or(1.0);
                    vec[idx] = count as f64 * idf_val;
                }
            }
            self.doc_vecs.push(vec);
        }
    }

    pub fn search(&self, query_tokens: &[String], top_k: usize) -> Vec<(String, f64)> {
        if self.doc_vecs.is_empty() {
            return Vec::new();
        }

        // Build query vector
        let mut query_vec = vec![0.0; self.vocab.len()];
        for token in query_tokens {
            if let Some(&idx) = self.vocab.get(token) {
                let idf_val = self.idf.get(token).copied().unwrap_or(1.0);
                query_vec[idx] = idf_val;
            }
        }

        // Compute cosine similarity
        let query_norm = norm(&query_vec).max(1.0);
        let mut results = Vec::new();

        for (i, doc_vec) in self.doc_vecs.iter().enumerate() {
            let dot = dot_product(&query_vec, doc_vec);
            let doc_norm = norm(doc_vec).max(1.0);
            let score = dot / (query_norm * doc_norm);
            if score > 0.0 {
                results.push((self.doc_paths[i].clone(), score));
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }
}

fn dot_product(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tfidf_search() {
        let mut tfidf = SimpleTFIDF::new();
        let docs = vec![
            (
                "doc1".to_string(),
                vec!["rust".to_string(), "code".to_string()],
            ),
            (
                "doc2".to_string(),
                vec!["python".to_string(), "code".to_string()],
            ),
        ];
        tfidf.index(docs);

        let results = tfidf.search(&["rust".to_string()], 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "doc1");
    }
}
