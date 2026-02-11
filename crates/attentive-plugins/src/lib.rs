//! Plugin system for custom routing strategies

pub mod base;
pub mod burnrate;
pub mod loopbreaker;
pub mod registry;
pub mod verifyfirst;

pub use base::{Plugin, SessionState, ToolCall};
pub use burnrate::BurnRatePlugin;
pub use loopbreaker::LoopBreakerPlugin;
pub use registry::PluginRegistry;
pub use verifyfirst::VerifyFirstPlugin;
