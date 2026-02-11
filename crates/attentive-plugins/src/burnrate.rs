//! BurnRate Plugin - Predicts and warns about rate limit consumption

use crate::base::{Plugin, SessionState, ToolCall, load_state, save_state};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const SAMPLE_WINDOW: usize = 20;
const WARNING_THRESHOLD_MINUTES: f64 = 30.0;
const CRITICAL_THRESHOLD_MINUTES: f64 = 10.0;

#[derive(Debug, Serialize, Deserialize, Default)]
struct BurnRateState {
    samples: VecDeque<Sample>,
    plan_type: String,
    warnings_issued: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sample {
    timestamp: String,
    session_tokens: u64,
}

#[derive(Debug)]
struct RateInfo {
    tokens_per_minute: f64,
    tokens_used: u64,
    limit: u64,
    minutes_remaining: Option<f64>,
}

pub struct BurnRatePlugin {
    name: String,
}

impl BurnRatePlugin {
    pub fn new() -> Self {
        Self {
            name: "burnrate".to_string(),
        }
    }

    fn stats_cache_path() -> Option<std::path::PathBuf> {
        let paths = attentive_telemetry::Paths::new().ok()?;
        Some(paths.home_claude.join("stats-cache.json"))
    }

    fn read_stats_cache() -> Option<serde_json::Value> {
        let path = Self::stats_cache_path()?;
        if !path.exists() {
            return None;
        }

        let contents = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    fn detect_plan_type(stats: &serde_json::Value) -> String {
        let model = stats.get("model").and_then(|m| m.as_str()).unwrap_or("");

        if model.is_empty() || model.contains("api") {
            return "api".to_string();
        }

        let session_tokens = stats
            .get("sessionTokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);

        if session_tokens > 300_000 {
            "max_20x".to_string()
        } else if session_tokens > 100_000 {
            "max_5x".to_string()
        } else {
            "pro".to_string()
        }
    }

    fn plan_limit(plan_type: &str) -> u64 {
        match plan_type {
            "free" => 25_000,
            "pro" => 150_000,
            "max_5x" => 500_000,
            "max_20x" => 2_000_000,
            _ => 150_000,
        }
    }

    fn record_sample(state: &mut BurnRateState, stats: &serde_json::Value) {
        let session_tokens = stats
            .get("sessionTokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);

        let sample = Sample {
            timestamp: chrono::Utc::now().to_rfc3339(),
            session_tokens,
        };

        state.samples.push_back(sample);

        while state.samples.len() > SAMPLE_WINDOW {
            state.samples.pop_front();
        }
    }

    fn calculate_burn_rate(state: &BurnRateState, stats: &serde_json::Value) -> Option<RateInfo> {
        if state.samples.len() < 2 {
            return None;
        }

        let first = state.samples.front()?;
        let last = state.samples.back()?;

        let first_time = chrono::DateTime::parse_from_rfc3339(&first.timestamp).ok()?;
        let last_time = chrono::DateTime::parse_from_rfc3339(&last.timestamp).ok()?;

        let elapsed_minutes = (last_time - first_time).num_seconds() as f64 / 60.0;

        if elapsed_minutes < 0.5 {
            return None; // Not enough time elapsed
        }

        let tokens_consumed = last.session_tokens.saturating_sub(first.session_tokens);

        if tokens_consumed == 0 {
            return None;
        }

        let tokens_per_minute = tokens_consumed as f64 / elapsed_minutes;

        let limit = Self::plan_limit(&state.plan_type);
        let session_tokens = stats
            .get("sessionTokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);

        let tokens_remaining = limit.saturating_sub(session_tokens);

        let minutes_remaining = if tokens_remaining > 0 && tokens_per_minute > 0.0 {
            Some(tokens_remaining as f64 / tokens_per_minute)
        } else {
            Some(0.0)
        };

        Some(RateInfo {
            tokens_per_minute,
            tokens_used: session_tokens,
            limit,
            minutes_remaining,
        })
    }
}

impl Default for BurnRatePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for BurnRatePlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_session_start(&mut self, _session_state: &SessionState) -> Option<String> {
        let stats = Self::read_stats_cache()?;
        let plan_type = Self::detect_plan_type(&stats);

        let mut state = BurnRateState {
            samples: VecDeque::new(),
            plan_type: plan_type.clone(),
            warnings_issued: 0,
        };

        Self::record_sample(&mut state, &stats);
        if let Err(e) = save_state(self.name(), &state) {
            eprintln!("BurnRate: failed to save state: {e}");
        }

        let session_tokens = stats.get("sessionTokens")?.as_u64()?;
        let limit = Self::plan_limit(&plan_type);

        if plan_type == "api" {
            Some("BurnRate: Active (API mode - per-minute limits)".to_string())
        } else {
            let pct = (session_tokens as f64 / limit as f64 * 100.0) as u64;
            Some(format!(
                "BurnRate: Active ({} plan, {}% used this window)",
                plan_type, pct
            ))
        }
    }

    fn on_prompt_post(
        &mut self,
        _prompt: &str,
        _context_output: &str,
        _session_state: &SessionState,
    ) -> String {
        let mut state: BurnRateState = load_state(self.name()).unwrap_or_default();
        let stats = match Self::read_stats_cache() {
            Some(s) => s,
            None => return String::new(),
        };

        Self::record_sample(&mut state, &stats);

        let rate_info = match Self::calculate_burn_rate(&state, &stats) {
            Some(r) => r,
            None => {
                save_state(self.name(), &state).ok();
                return String::new();
            }
        };

        let minutes_remaining = match rate_info.minutes_remaining {
            Some(m) if m.is_finite() => m,
            _ => {
                save_state(self.name(), &state).ok();
                return String::new();
            }
        };

        let level = if minutes_remaining <= CRITICAL_THRESHOLD_MINUTES {
            state.warnings_issued += 1;
            "CRITICAL"
        } else if minutes_remaining <= WARNING_THRESHOLD_MINUTES {
            state.warnings_issued += 1;
            "WARNING"
        } else {
            save_state(self.name(), &state).ok();
            return String::new();
        };

        save_state(self.name(), &state).ok();

        format!(
            "\n## BurnRate {}\n\
            **Estimated time until rate limit: ~{} minutes**\n\
            \n\
            - Current burn rate: {:.0} tokens/min\n\
            - Tokens used this window: {}\n\
            - Window limit: {}\n\
            {}",
            level,
            minutes_remaining as i32,
            rate_info.tokens_per_minute,
            rate_info.tokens_used,
            rate_info.limit,
            if level == "CRITICAL" {
                "\n**Consider:**\n\
                - Pausing for a few minutes to let the window slide\n\
                - Switching to a smaller model (Haiku) for simple tasks\n\
                - Breaking work into smaller, focused prompts\n"
            } else {
                ""
            }
        )
    }

    fn on_stop(
        &mut self,
        _tool_calls: &[ToolCall],
        _session_state: &SessionState,
    ) -> Option<String> {
        let mut state: BurnRateState = load_state(self.name()).unwrap_or_default();
        let stats = Self::read_stats_cache()?;

        Self::record_sample(&mut state, &stats);
        save_state(self.name(), &state).ok();

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_limits() {
        assert_eq!(BurnRatePlugin::plan_limit("free"), 25_000);
        assert_eq!(BurnRatePlugin::plan_limit("pro"), 150_000);
        assert_eq!(BurnRatePlugin::plan_limit("max_5x"), 500_000);
        assert_eq!(BurnRatePlugin::plan_limit("max_20x"), 2_000_000);
    }

    #[test]
    fn test_detect_plan_type() {
        let stats_pro = serde_json::json!({
            "sessionTokens": 50000,
            "model": "claude-opus"
        });
        assert_eq!(BurnRatePlugin::detect_plan_type(&stats_pro), "pro");

        let stats_max5 = serde_json::json!({
            "sessionTokens": 200000,
            "model": "claude-opus"
        });
        assert_eq!(BurnRatePlugin::detect_plan_type(&stats_max5), "max_5x");

        let stats_max20 = serde_json::json!({
            "sessionTokens": 500000,
            "model": "claude-opus"
        });
        assert_eq!(BurnRatePlugin::detect_plan_type(&stats_max20), "max_20x");
    }
}
