//! JSONL I/O and atomic file operations

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// Append a JSON record to a JSONL file
pub fn append_jsonl<T: Serialize>(path: &Path, record: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    let json = serde_json::to_string(record)?;
    writeln!(file, "{}", json)?;
    Ok(())
}

/// Read all records from a JSONL file
pub fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> std::io::Result<Vec<T>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(&line) {
            Ok(record) => records.push(record),
            Err(_) => continue, // Skip malformed lines
        }
    }

    Ok(records)
}

/// Write data atomically using temp file + rename
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, data)?;
    std::fs::rename(temp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestRecord {
        id: u32,
        name: String,
    }

    #[test]
    fn test_jsonl_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_roundtrip.jsonl");

        // Clean up if exists
        let _ = std::fs::remove_file(&test_file);

        let records = vec![
            TestRecord {
                id: 1,
                name: "Alice".to_string(),
            },
            TestRecord {
                id: 2,
                name: "Bob".to_string(),
            },
        ];

        // Append records
        for record in &records {
            append_jsonl(&test_file, record).unwrap();
        }

        // Read back
        let read_records: Vec<TestRecord> = read_jsonl(&test_file).unwrap();
        assert_eq!(records, read_records);

        // Clean up
        std::fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn test_atomic_write() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_atomic.txt");

        let data = b"Hello, world!";
        atomic_write(&test_file, data).unwrap();

        let read_data = std::fs::read(&test_file).unwrap();
        assert_eq!(data, read_data.as_slice());

        // Clean up
        std::fs::remove_file(&test_file).unwrap();
    }
}
