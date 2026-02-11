use attentive_index::{Document, SearchIndex};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use tempfile::TempDir;

fn bench_index_search_100_docs(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut index = SearchIndex::new(&db_path).unwrap();

    let mut documents = Vec::new();
    for i in 0..100 {
        documents.push(Document {
            path: format!("file{}.rs", i),
            content: format!("function test{} implementation with keywords", i),
            mtime: 0.0,
            doc_type: "rust".to_string(),
        });
    }

    index.build(documents).unwrap();

    c.bench_function("index_search_100_docs", |b| {
        b.iter(|| {
            index
                .query(black_box("function implementation"), 10)
                .unwrap();
        });
    });
}

criterion_group!(benches, bench_index_search_100_docs);
criterion_main!(benches);
