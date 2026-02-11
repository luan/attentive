//! SearchIndex with SQLite storage and hybrid search

use crate::bm25::BM25;
use crate::tfidf::SimpleTFIDF;
use anyhow::Result;
use chrono::Utc;
use regex::Regex;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

static TOKENIZE_RE: OnceLock<Regex> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct Document {
    pub path: String,
    pub content: String,
    pub mtime: f64,
    pub doc_type: String,
}

pub struct SearchIndex {
    db_path: PathBuf,
    bm25: Option<BM25>,
    tfidf: Option<SimpleTFIDF>,
}

impl SearchIndex {
    pub fn new(db_path: impl Into<PathBuf>) -> Result<Self> {
        let db_path = db_path.into();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let index = Self {
            db_path,
            bm25: None,
            tfidf: None,
        };

        index.init_db()?;
        Ok(index)
    }

    fn init_db(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS documents (
                path TEXT PRIMARY KEY,
                content TEXT,
                outline TEXT,
                mtime REAL,
                doc_type TEXT,
                indexed_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_mtime ON documents(mtime)",
            [],
        )?;
        Ok(())
    }

    pub fn build(&mut self, documents: Vec<Document>) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;

        // Clear existing data
        conn.execute("DELETE FROM documents", [])?;

        // Insert documents
        for doc in &documents {
            conn.execute(
                "INSERT INTO documents (path, content, outline, mtime, doc_type, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    &doc.path,
                    &doc.content,
                    "",
                    doc.mtime,
                    &doc.doc_type,
                    Utc::now().to_rfc3339()
                ],
            )?;
        }

        // Rebuild in-memory index
        self.rebuild_memory_index()?;

        Ok(())
    }

    pub fn update_incremental(&mut self, documents: Vec<Document>) -> Result<usize> {
        let conn = Connection::open(&self.db_path)?;

        // Get existing mtimes
        let mut existing: HashMap<String, f64> = HashMap::new();
        let mut stmt = conn.prepare("SELECT path, mtime FROM documents")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        for row in rows {
            let (path, mtime): (String, f64) = row?;
            existing.insert(path, mtime);
        }

        // Update only changed documents
        let mut updated = 0;
        for doc in documents {
            let should_update = existing
                .get(&doc.path)
                .map(|&old_mtime| old_mtime < doc.mtime)
                .unwrap_or(true);

            if should_update {
                conn.execute(
                    "INSERT OR REPLACE INTO documents (path, content, outline, mtime, doc_type, indexed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        &doc.path,
                        &doc.content,
                        "",
                        doc.mtime,
                        &doc.doc_type,
                        Utc::now().to_rfc3339()
                    ],
                )?;
                updated += 1;
            }
        }

        if updated > 0 {
            self.rebuild_memory_index()?;
        }

        Ok(updated)
    }

    fn rebuild_memory_index(&mut self) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare("SELECT path, content FROM documents")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut documents = Vec::new();
        for row in rows {
            let (path, content) = row?;
            documents.push((path, content));
        }

        if documents.is_empty() {
            self.bm25 = None;
            self.tfidf = None;
            return Ok(());
        }

        // Tokenize documents
        let tokenized: Vec<_> = documents
            .iter()
            .map(|(path, content)| (path.clone(), tokenize(content)))
            .collect();

        // Build BM25 index
        let mut bm25 = BM25::new();
        bm25.index(tokenized.clone());
        self.bm25 = Some(bm25);

        // Also build TF-IDF fallback
        let mut tfidf = SimpleTFIDF::new();
        tfidf.index(tokenized);
        self.tfidf = Some(tfidf);

        Ok(())
    }

    fn get_document_contents(&self) -> Result<HashMap<String, String>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare("SELECT path, content FROM documents")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut contents = HashMap::new();
        for row in rows {
            let (path, content) = row?;
            contents.insert(path, content);
        }
        Ok(contents)
    }

    pub fn query(&self, prompt: &str, top_k: usize) -> Result<Vec<(String, f64)>> {
        // Ensure index is loaded
        if self.bm25.is_none() && self.tfidf.is_none() {
            return Ok(Vec::new());
        }

        let query_tokens = tokenize(prompt);

        // Try BM25 first, fallback to TF-IDF
        let results = if let Some(bm25) = &self.bm25 {
            bm25.search(&query_tokens, top_k * 3) // Get more candidates for reranking
        } else if let Some(tfidf) = &self.tfidf {
            tfidf.search(&query_tokens, top_k * 3)
        } else {
            Vec::new()
        };

        // Apply semantic reranking
        let contents = self.get_document_contents()?;
        let reranked = semantic_rerank(prompt, results, &contents, top_k);
        Ok(reranked)
    }

    pub fn get_stats(&self) -> Result<HashMap<String, serde_json::Value>> {
        let conn = Connection::open(&self.db_path)?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))?;

        let mut stats = HashMap::new();
        stats.insert(
            "total_documents".to_string(),
            serde_json::Value::Number(count.into()),
        );
        stats.insert(
            "bm25_available".to_string(),
            serde_json::Value::Bool(self.bm25.is_some()),
        );

        Ok(stats)
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let re = TOKENIZE_RE.get_or_init(|| Regex::new(r"[a-z][a-z0-9_]{2,}").unwrap());
    re.find_iter(&text.to_lowercase())
        .map(|m| m.as_str().to_string())
        .collect()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < 1e-8 || norm_b < 1e-8 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

fn semantic_rerank(
    query: &str,
    candidates: Vec<(String, f64)>,
    contents: &std::collections::HashMap<String, String>,
    top_k: usize,
) -> Vec<(String, f64)> {
    use fastembed::TextEmbedding;

    let mut model = match TextEmbedding::try_new(Default::default()) {
        Ok(m) => m,
        Err(_) => return candidates.into_iter().take(top_k).collect(),
    };

    let query_emb = match model.embed(vec![query.to_string()], None) {
        Ok(v) if !v.is_empty() => v[0].clone(),
        _ => return candidates.into_iter().take(top_k).collect(),
    };

    let bm25_max = candidates.iter().map(|(_, s)| *s).fold(0.0f64, f64::max);

    let mut scored: Vec<(String, f64)> = candidates
        .iter()
        .filter_map(|(path, bm25_score)| {
            let content = contents.get(path)?;
            let truncated = if content.len() > 2000 {
                &content[..2000]
            } else {
                content.as_str()
            };
            let doc_emb = model
                .embed(vec![truncated.to_string()], None)
                .ok()?
                .into_iter()
                .next()?;
            let sim = cosine_similarity(&query_emb, &doc_emb) as f64;
            let norm_bm25 = if bm25_max > 0.0 {
                bm25_score / bm25_max
            } else {
                0.0
            };
            let combined = 0.6 * norm_bm25 + 0.4 * sim;
            Some((path.clone(), combined))
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(top_k).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &c).abs() < 1e-6); // orthogonal
    }

    #[test]
    fn test_semantic_rerank_basic() {
        // This test only runs when embeddings feature is enabled
        // Requires model download on first run
        let model = fastembed::TextEmbedding::try_new(Default::default());
        assert!(model.is_ok(), "Failed to load embedding model");
    }

    #[test]
    fn test_empty_corpus() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_empty.db");
        let _ = std::fs::remove_file(&db_path);

        let mut index = SearchIndex::new(&db_path).unwrap();
        index.build(vec![]).unwrap();

        let results = index.query("test", 10).unwrap();
        assert_eq!(results.len(), 0);

        std::fs::remove_file(&db_path).unwrap();
    }

    #[test]
    fn test_bm25_ranks_relevant() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_bm25.db");
        let _ = std::fs::remove_file(&db_path);

        let mut index = SearchIndex::new(&db_path).unwrap();
        let docs = vec![
            Document {
                path: "rust_guide.md".to_string(),
                content: "Rust is a systems programming language".to_string(),
                mtime: 1.0,
                doc_type: "markdown".to_string(),
            },
            Document {
                path: "python_guide.md".to_string(),
                content: "Python is a high-level programming language".to_string(),
                mtime: 1.0,
                doc_type: "markdown".to_string(),
            },
        ];

        index.build(docs).unwrap();

        let results = index.query("rust programming", 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].0, "rust_guide.md");

        std::fs::remove_file(&db_path).unwrap();
    }

    #[test]
    fn test_incremental_update() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_incremental.db");
        let _ = std::fs::remove_file(&db_path);

        let mut index = SearchIndex::new(&db_path).unwrap();

        // Initial build
        let docs = vec![Document {
            path: "doc1.md".to_string(),
            content: "initial content".to_string(),
            mtime: 1.0,
            doc_type: "markdown".to_string(),
        }];
        index.build(docs).unwrap();

        // Incremental update (same mtime - should not update)
        let docs = vec![Document {
            path: "doc1.md".to_string(),
            content: "initial content".to_string(),
            mtime: 1.0,
            doc_type: "markdown".to_string(),
        }];
        let updated = index.update_incremental(docs).unwrap();
        assert_eq!(updated, 0);

        // Incremental update (newer mtime - should update)
        let docs = vec![Document {
            path: "doc1.md".to_string(),
            content: "updated content".to_string(),
            mtime: 2.0,
            doc_type: "markdown".to_string(),
        }];
        let updated = index.update_incremental(docs).unwrap();
        assert_eq!(updated, 1);

        std::fs::remove_file(&db_path).unwrap();
    }
}
