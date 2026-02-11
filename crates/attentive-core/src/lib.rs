//! Core context routing algorithms and advisor logic

mod config;
mod router;
mod types;

pub use config::{Config, DecayRates};
pub use router::Router;
pub use types::{AttentionState, Tier};
