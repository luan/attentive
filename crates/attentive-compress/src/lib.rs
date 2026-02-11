//! Context compression using LLM-based summarization

mod compress;
pub mod compressor;
mod storage;
mod types;

pub use compress::fallback_compress;
pub use compressor::CompressResult;
pub use storage::ObservationDb;
pub use types::{CompressedObservation, ObservationIndex};
