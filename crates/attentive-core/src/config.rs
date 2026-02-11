//! Configuration for attention routing

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Decay rates per category
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayRates {
    pub rates: HashMap<String, f64>,
    pub default: f64,
}

impl DecayRates {
    pub fn new() -> Self {
        let mut rates = HashMap::new();
        rates.insert("systems/".to_string(), 0.85);
        rates.insert("modules/".to_string(), 0.70);
        rates.insert("integrations/".to_string(), 0.80);
        rates.insert("docs/".to_string(), 0.75);

        Self {
            rates,
            default: 0.70,
        }
    }

    pub fn get_decay(&self, path: &str) -> f64 {
        for (prefix, &rate) in &self.rates {
            if path.starts_with(prefix) {
                return rate;
            }
        }
        self.default
    }
}

impl Default for DecayRates {
    fn default() -> Self {
        Self::new()
    }
}

/// Router configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Decay rates per category
    pub decay_rates: DecayRates,

    /// HOT threshold (>= this = full file injection)
    pub hot_threshold: f64,

    /// WARM threshold (>= this = TOC injection)
    pub warm_threshold: f64,

    /// Co-activation boost (related files)
    pub coactivation_boost: f64,

    /// Transitive co-activation boost (2-hop)
    pub transitive_boost: f64,

    /// Max HOT files
    pub max_hot_files: usize,

    /// Max WARM files
    pub max_warm_files: usize,

    /// Pinned file floor boost
    pub pinned_floor_boost: f64,

    /// Demoted file penalty multiplier
    pub demoted_penalty: f64,

    /// Co-activation graph (file -> related files)
    pub co_activation: HashMap<String, Vec<String>>,

    /// Pinned files (always at least WARM)
    pub pinned_files: Vec<String>,

    /// Demoted files (penalty applied)
    pub demoted_files: Vec<String>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            decay_rates: DecayRates::new(),
            hot_threshold: 0.8,
            warm_threshold: 0.25,
            coactivation_boost: 0.35,
            transitive_boost: 0.15,
            max_hot_files: 3,
            max_warm_files: 5,
            pinned_floor_boost: 0.1,
            demoted_penalty: 0.5,
            co_activation: HashMap::new(),
            pinned_files: Vec::new(),
            demoted_files: Vec::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decay_rates() {
        let rates = DecayRates::new();
        assert_eq!(rates.get_decay("systems/core.md"), 0.85);
        assert_eq!(rates.get_decay("modules/api.md"), 0.70);
        assert_eq!(rates.get_decay("unknown/file.md"), 0.70);
    }

    #[test]
    fn test_config_defaults() {
        let config = Config::new();
        assert_eq!(config.hot_threshold, 0.8);
        assert_eq!(config.warm_threshold, 0.25);
        assert_eq!(config.max_hot_files, 3);
    }
}
