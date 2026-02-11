use crate::{CompressedObservation, ObservationIndex};
use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct ObservationDb {
    conn: Connection,
}

impl ObservationDb {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS observations (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                observation_type TEXT NOT NULL,
                concepts TEXT NOT NULL,
                raw_tokens INTEGER NOT NULL,
                compressed_tokens INTEGER NOT NULL,
                semantic_summary TEXT NOT NULL,
                key_facts TEXT NOT NULL,
                related_files TEXT NOT NULL,
                raw_content_hash TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_session ON observations(session_id);
            CREATE INDEX IF NOT EXISTS idx_timestamp ON observations(timestamp);
            CREATE VIRTUAL TABLE IF NOT EXISTS observations_fts USING fts5(
                id,
                semantic_summary,
                key_facts,
                concepts,
                content=observations,
                content_rowid=rowid
            );
            CREATE TRIGGER IF NOT EXISTS observations_ai AFTER INSERT ON observations BEGIN
                INSERT INTO observations_fts(rowid, id, semantic_summary, key_facts, concepts)
                VALUES (new.rowid, new.id, new.semantic_summary, new.key_facts, new.concepts);
            END;
            ",
        )?;
        Ok(())
    }

    pub fn insert(&self, obs: &CompressedObservation) -> Result<()> {
        self.conn.execute(
            "INSERT INTO observations VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                obs.id,
                obs.session_id,
                obs.timestamp.to_rfc3339(),
                obs.tool_name,
                obs.observation_type,
                serde_json::to_string(&obs.concepts)?,
                obs.raw_tokens,
                obs.compressed_tokens,
                obs.semantic_summary,
                serde_json::to_string(&obs.key_facts)?,
                serde_json::to_string(&obs.related_files)?,
                obs.raw_content_hash,
            ],
        )?;
        Ok(())
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<CompressedObservation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM observations WHERE id = ?")?;
        let mut rows = stmt.query(params![id])?;

        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_observation(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<CompressedObservation>> {
        let escaped = query.replace('"', "\"\"");
        let fts_query = format!("\"{}\"", escaped);

        let mut stmt = self.conn.prepare(
            "SELECT o.* FROM observations o
             JOIN observations_fts f ON o.id = f.id
             WHERE observations_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let mut results = Vec::new();
        let rows = stmt.query_map(params![fts_query, limit as i64], |row| {
            Self::row_to_observation(row).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )),
                )
            })
        })?;

        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn get_index(&self) -> Result<Vec<ObservationIndex>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, observation_type, semantic_summary, compressed_tokens, concepts
             FROM observations ORDER BY timestamp DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let date: String = row.get(1)?;
            let concepts_str: String = row.get(5)?;
            let concepts: Vec<String> = serde_json::from_str(&concepts_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to parse concepts JSON: {}", e),
                    )),
                )
            })?;
            Ok(ObservationIndex {
                id: row.get(0)?,
                date: date[..10].to_string(),
                obs_type: row.get(2)?,
                title: row.get(3)?,
                token_count: row.get(4)?,
                concepts,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_timeline(&self, obs_id: &str, window: usize) -> Result<Vec<CompressedObservation>> {
        let target_ts: String = self.conn.query_row(
            "SELECT timestamp FROM observations WHERE id = ?",
            params![obs_id],
            |row| row.get(0),
        )?;

        let mut stmt = self.conn.prepare(
            "SELECT * FROM observations
             WHERE abs(julianday(timestamp) - julianday(?1)) <= ?2
             ORDER BY timestamp",
        )?;
        let rows = stmt.query_map(params![target_ts, window as f64], |row| {
            Self::row_to_observation(row).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )),
                )
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn row_to_observation(row: &rusqlite::Row) -> Result<CompressedObservation> {
        Ok(CompressedObservation {
            id: row.get(0)?,
            session_id: row.get(1)?,
            timestamp: row.get::<_, String>(2)?.parse()?,
            tool_name: row.get(3)?,
            observation_type: row.get(4)?,
            concepts: serde_json::from_str(&row.get::<_, String>(5)?)?,
            raw_tokens: row.get(6)?,
            compressed_tokens: row.get(7)?,
            semantic_summary: row.get(8)?,
            key_facts: serde_json::from_str(&row.get::<_, String>(9)?)?,
            related_files: serde_json::from_str(&row.get::<_, String>(10)?)?,
            raw_content_hash: row.get(11)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_observation(id: &str, summary: &str) -> CompressedObservation {
        CompressedObservation {
            id: id.to_string(),
            session_id: "sess_1".to_string(),
            timestamp: Utc::now(),
            tool_name: "bash".to_string(),
            observation_type: "bugfix".to_string(),
            concepts: vec!["testing".to_string()],
            raw_tokens: 100,
            compressed_tokens: 50,
            semantic_summary: summary.to_string(),
            key_facts: vec!["fact1".to_string()],
            related_files: vec!["test.rs".to_string()],
            raw_content_hash: "abc123".to_string(),
        }
    }

    #[test]
    fn test_db_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_obs.db");
        let _ = std::fs::remove_file(&db_path);

        let db = ObservationDb::new(&db_path).unwrap();

        let obs = CompressedObservation {
            id: "obs_test".to_string(),
            session_id: "sess_1".to_string(),
            timestamp: Utc::now(),
            tool_name: "bash".to_string(),
            observation_type: "bugfix".to_string(),
            concepts: vec!["testing".to_string()],
            raw_tokens: 100,
            compressed_tokens: 50,
            semantic_summary: "Test summary".to_string(),
            key_facts: vec!["fact1".to_string()],
            related_files: vec!["test.rs".to_string()],
            raw_content_hash: "abc123".to_string(),
        };

        db.insert(&obs).unwrap();
        let retrieved = db.get_by_id("obs_test").unwrap().unwrap();
        assert_eq!(retrieved.id, "obs_test");

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_fts5_search() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_fts5.db");
        let _ = std::fs::remove_file(&db_path);

        let db = ObservationDb::new(&db_path).unwrap();

        let obs1 = test_observation("obs1", "Fixed authentication bug in login flow");
        let obs2 = test_observation("obs2", "Added new database migration for users table");
        let obs3 = test_observation("obs3", "Refactored authentication middleware");

        db.insert(&obs1).unwrap();
        db.insert(&obs2).unwrap();
        db.insert(&obs3).unwrap();

        let results = db.search("authentication", 10).unwrap();
        assert!(results.len() >= 2);
        let ids: Vec<_> = results.iter().map(|o| o.id.as_str()).collect();
        assert!(ids.contains(&"obs1"));
        assert!(ids.contains(&"obs3"));

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_get_index() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_index.db");
        let _ = std::fs::remove_file(&db_path);

        let db = ObservationDb::new(&db_path).unwrap();
        db.insert(&test_observation("obs1", "summary one")).unwrap();
        db.insert(&test_observation("obs2", "summary two")).unwrap();

        let index = db.get_index().unwrap();
        assert_eq!(index.len(), 2);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_search_handles_no_results_gracefully() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_search_empty.db");
        let _ = std::fs::remove_file(&db_path);

        let db = ObservationDb::new(&db_path).unwrap();
        db.insert(&test_observation("obs1", "authentication fix"))
            .unwrap();

        // Search for something not in the DB
        let results = db.search("nonexistent_term_xyz", 10).unwrap();
        assert!(results.is_empty());

        let _ = std::fs::remove_file(&db_path);
    }
}
