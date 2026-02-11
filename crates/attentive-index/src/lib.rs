//! BM25 + SQLite search index

mod bm25;
mod index;
mod tfidf;

pub use index::{Document, SearchIndex};
