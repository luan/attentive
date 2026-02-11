use attentive_core::{AttentionState, Config, Router};
use attentive_learn::Learner;
use attentive_telemetry::Paths;
use std::path::Path;
use std::time::Instant;

struct BenchmarkResult {
    repo_path: String,
    files_scanned: usize,
    baseline_tokens: usize,
    attentive_tokens: usize,
    reduction_pct: f64,
    router_latency_us: u128,
    context_build_latency_us: u128,
    hot_count: usize,
    warm_count: usize,
    cold_count: usize,
    hot_chars: usize,
    warm_chars: usize,
}

fn scan_repo_files(root: &Path) -> Vec<(String, String)> {
    let skip_dirs = [
        ".git",
        "node_modules",
        "target",
        "__pycache__",
        ".venv",
        "dist",
        "build",
    ];
    let mut files = Vec::new();
    scan_dir(root, root, &skip_dirs, &mut files);
    files
}

fn scan_dir(root: &Path, dir: &Path, skip: &[&str], files: &mut Vec<(String, String)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if !skip.contains(&name.as_str()) {
                scan_dir(root, &path, skip, files);
            }
        } else if path.is_file()
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            files.push((rel, content));
        }
    }
}

fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

fn format_result(r: &BenchmarkResult) -> String {
    format!(
        "Attentive Benchmark\n===================\n\
         Repo: {}\n\
         Files scanned: {}\n\
         Baseline tokens: {:>10} (all files)\n\
         Attentive tokens: {:>9} (HOT + WARM)\n\
         Reduction: {:.1}%\n\n\
         Latency:\n\
         {:>8}μs  router update\n\
         {:>8}μs  context build\n\
         {:>8}μs  total\n\n\
         Context:\n\
         {:>4} HOT  ({:>6} chars)\n\
         {:>4} WARM ({:>6} chars)\n\
         {:>4} COLD (evicted)",
        r.repo_path,
        r.files_scanned,
        r.baseline_tokens,
        r.attentive_tokens,
        r.reduction_pct,
        r.router_latency_us,
        r.context_build_latency_us,
        r.router_latency_us + r.context_build_latency_us,
        r.hot_count,
        r.hot_chars,
        r.warm_count,
        r.warm_chars,
        r.cold_count,
    )
}

pub fn run() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;

    // 1. Scan repo
    let files = scan_repo_files(&cwd);
    if files.is_empty() {
        println!("No files found in {}", cwd.display());
        return Ok(());
    }

    // 2. Baseline: all file tokens
    let baseline_tokens: usize = files.iter().map(|(_, c)| estimate_tokens(c)).sum();

    // 3. Load learned state
    let paths = Paths::new()?;
    let learned_state_path = paths.learned_state_path()?;
    let learner = if learned_state_path.exists() {
        std::fs::read_to_string(&learned_state_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_else(Learner::new)
    } else {
        Learner::new()
    };

    // 4. Build attention state from file list
    let config = Config::default();
    let router = Router::new(config);
    let mut state = AttentionState::new();

    // Seed with absolute paths so learner lookups match
    for (path, _) in &files {
        let abs_path = cwd.join(path).to_string_lossy().to_string();
        state.scores.insert(abs_path, 0.5);
    }

    // 5. Time router update
    let start = Instant::now();
    let prompt = "benchmark run";
    router.update_attention(
        &mut state,
        prompt,
        Some(&learner),
        std::collections::HashSet::new(),
    );
    let router_us = start.elapsed().as_micros();

    // 6. Count tiers from raw state (before truncation)
    use attentive_core::Tier;
    let mut hot_count = 0usize;
    let mut warm_count = 0usize;
    let mut cold_count = 0usize;
    for &score in state.scores.values() {
        match Tier::from_score(score) {
            Tier::Hot => hot_count += 1,
            Tier::Warm => warm_count += 1,
            Tier::Cold => cold_count += 1,
        }
    }

    // 7. Time context build (applies truncation limits)
    let start = Instant::now();
    let (hot, warm, _cold) = router.build_context_output(&state);
    let context_us = start.elapsed().as_micros();

    // 8. Calculate output tokens (hot/warm paths are absolute, files are relative)
    let file_map: std::collections::HashMap<String, &str> = files
        .iter()
        .map(|(rel, content)| {
            (
                cwd.join(rel).to_string_lossy().to_string(),
                content.as_str(),
            )
        })
        .collect();
    let hot_chars: usize = hot
        .iter()
        .filter_map(|p| file_map.get(p))
        .map(|c| c.len())
        .sum();
    let warm_chars: usize = warm
        .iter()
        .filter_map(|p| file_map.get(p))
        .map(|c| c.len().min(500))
        .sum();
    let attentive_tokens = estimate_tokens(&" ".repeat(hot_chars + warm_chars));
    let reduction = if baseline_tokens > 0 {
        (1.0 - attentive_tokens as f64 / baseline_tokens as f64) * 100.0
    } else {
        0.0
    };

    let result = BenchmarkResult {
        repo_path: cwd.to_string_lossy().to_string(),
        files_scanned: files.len(),
        baseline_tokens,
        attentive_tokens,
        reduction_pct: reduction,
        router_latency_us: router_us,
        context_build_latency_us: context_us,
        hot_count,
        warm_count,
        cold_count,
        hot_chars,
        warm_chars,
    };

    println!("{}", format_result(&result));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_repo_files() {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::write(temp.path().join("a.rs"), "fn main() {}").unwrap();
        std::fs::write(temp.path().join("b.md"), "# Title").unwrap();
        std::fs::create_dir_all(temp.path().join(".git")).unwrap();
        std::fs::write(temp.path().join(".git/config"), "gitconfig").unwrap();

        let files = scan_repo_files(temp.path());
        assert_eq!(files.len(), 2); // .git excluded
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("hello world"), 2); // 11 chars / 4 = 2
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn test_benchmark_output_format() {
        let result = BenchmarkResult {
            repo_path: "/test".to_string(),
            files_scanned: 10,
            baseline_tokens: 50000,
            attentive_tokens: 5000,
            reduction_pct: 90.0,
            router_latency_us: 245,
            context_build_latency_us: 89,
            hot_count: 3,
            warm_count: 5,
            cold_count: 2,
            hot_chars: 12000,
            warm_chars: 4000,
        };
        let output = format_result(&result);
        assert!(output.contains("90.0%"));
        assert!(output.contains("50000")); // No comma separator in the format
    }
}
