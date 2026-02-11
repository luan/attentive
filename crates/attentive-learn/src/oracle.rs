//! Task classification and cost prediction

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Refactor,
    BugFix,
    Feature,
    Review,
    Exploration,
    Config,
}

struct TaskKeywords {
    task_type: TaskType,
    keywords: &'static [&'static str],
}

const TASK_KEYWORD_MAP: &[TaskKeywords] = &[
    TaskKeywords {
        task_type: TaskType::Refactor,
        keywords: &[
            "refactor",
            "rename",
            "reorganize",
            "restructure",
            "cleanup",
            "simplify",
            "extract",
            "move",
        ],
    },
    TaskKeywords {
        task_type: TaskType::BugFix,
        keywords: &[
            "fix", "bug", "error", "broken", "crash", "issue", "wrong", "fail", "problem",
        ],
    },
    TaskKeywords {
        task_type: TaskType::Feature,
        keywords: &[
            "add",
            "implement",
            "create",
            "new",
            "feature",
            "build",
            "develop",
        ],
    },
    TaskKeywords {
        task_type: TaskType::Review,
        keywords: &["review", "check", "examine", "audit", "analyze"],
    },
    TaskKeywords {
        task_type: TaskType::Exploration,
        keywords: &[
            "find", "search", "where", "how does", "what is", "explain", "show", "explore",
        ],
    },
    TaskKeywords {
        task_type: TaskType::Config,
        keywords: &[
            "config",
            "setting",
            "environment",
            "setup",
            "install",
            "deploy",
        ],
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    pub tokens: usize,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Oracle {
    task_costs: HashMap<String, CostEntry>,
}

impl Oracle {
    pub fn new() -> Self {
        Self {
            task_costs: HashMap::new(),
        }
    }

    pub fn classify_task(&self, prompt: &str) -> TaskType {
        let prompt_lower = prompt.to_lowercase();
        let mut best_match: Option<(TaskType, usize)> = None;

        for entry in TASK_KEYWORD_MAP {
            let count = entry
                .keywords
                .iter()
                .filter(|kw| prompt_lower.contains(*kw))
                .count();
            if count > 0 && (best_match.is_none() || count > best_match.unwrap().1) {
                best_match = Some((entry.task_type, count));
            }
        }

        best_match.map(|(t, _)| t).unwrap_or(TaskType::Feature)
    }

    pub fn record_cost(&mut self, task_type: TaskType, tokens: usize) {
        let key = format!("{:?}", task_type).to_lowercase();
        let entry = self.task_costs.entry(key).or_insert(CostEntry {
            tokens: 0,
            count: 0,
        });
        entry.tokens += tokens;
        entry.count += 1;
    }

    pub fn estimate_cost(&self, task_type: TaskType) -> Option<usize> {
        let key = format!("{:?}", task_type).to_lowercase();
        self.task_costs
            .get(&key)
            .map(|e| if e.count > 0 { e.tokens / e.count } else { 0 })
    }
}

impl Default for Oracle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_bugfix() {
        let oracle = Oracle::new();
        assert_eq!(
            oracle.classify_task("fix the broken login"),
            TaskType::BugFix
        );
    }

    #[test]
    fn test_classify_refactor() {
        let oracle = Oracle::new();
        assert_eq!(
            oracle.classify_task("refactor the module"),
            TaskType::Refactor
        );
    }

    #[test]
    fn test_classify_feature() {
        let oracle = Oracle::new();
        assert_eq!(oracle.classify_task("add a new feature"), TaskType::Feature);
    }

    #[test]
    fn test_classify_exploration() {
        let oracle = Oracle::new();
        assert_eq!(
            oracle.classify_task("explore the codebase"),
            TaskType::Exploration
        );
    }

    #[test]
    fn test_classify_unknown_defaults_feature() {
        let oracle = Oracle::new();
        assert_eq!(oracle.classify_task("hello world"), TaskType::Feature);
    }

    #[test]
    fn test_cost_tracking() {
        let mut oracle = Oracle::new();
        oracle.record_cost(TaskType::BugFix, 1000);
        oracle.record_cost(TaskType::BugFix, 2000);
        assert_eq!(oracle.estimate_cost(TaskType::BugFix), Some(1500));
    }

    #[test]
    fn test_json_roundtrip() {
        let mut oracle = Oracle::new();
        oracle.record_cost(TaskType::Feature, 5000);
        let json = serde_json::to_string(&oracle).unwrap();
        let loaded: Oracle = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.estimate_cost(TaskType::Feature), Some(5000));
    }
}
