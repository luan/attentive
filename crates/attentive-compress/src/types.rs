use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedObservation {
    pub id: String,
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub tool_name: String,
    pub observation_type: String,
    pub concepts: Vec<String>,
    pub raw_tokens: i64,
    pub compressed_tokens: i64,
    pub semantic_summary: String,
    pub key_facts: Vec<String>,
    pub related_files: Vec<String>,
    pub raw_content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationIndex {
    pub id: String,
    pub date: String,
    pub obs_type: String,
    pub title: String,
    pub token_count: i64,
    pub concepts: Vec<String>,
}
