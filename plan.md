# Task 1: Init workspace skeleton + CI
type: task
priority: 1
parent: attentive-qyj
labels: architecture

## Description
Create Cargo workspace with 8 crate stubs. Each crate has Cargo.toml with correct dependencies, lib.rs or main.rs, and compiles. Add .gitignore, rustfmt.toml, clippy.toml. Add GitHub Actions CI (cargo check, test, clippy, fmt).

## Test
```
cargo check --workspace
cargo test --workspace
```

## Files
- Cargo.toml (workspace root)
- crates/attentive/Cargo.toml + src/main.rs
- crates/attentive-core/Cargo.toml + src/lib.rs
- crates/attentive-learn/Cargo.toml + src/lib.rs
- crates/attentive-index/Cargo.toml + src/lib.rs
- crates/attentive-repo/Cargo.toml + src/lib.rs
- crates/attentive-compress/Cargo.toml + src/lib.rs
- crates/attentive-plugins/Cargo.toml + src/lib.rs
- crates/attentive-telemetry/Cargo.toml + src/lib.rs
- .gitignore, rustfmt.toml

---

# Task 2: attentive-telemetry — shared types and paths
type: task
priority: 1
parent: attentive-qyj
labels: implementation

## Description
Port telemetry_lib.py + telemetry_record.py + telemetry_report.py. Core shared types: Paths (with git worktree detection), TurnRecord, AttentionHistoryEntry, estimate_tokens(), ContentType enum, JSONL read/write, atomic file writes (temp+rename).

## Test
- Paths resolves ~/.claude, project .claude, worktree common dir
- TurnRecord round-trips through serde_json
- estimate_tokens matches Python within 10%
- Atomic write survives simulated crash (write to temp, rename)
- JSONL append + read back

## Key types
```rust
pub struct Paths { claude_dir, telemetry_dir, project_claude_dir }
pub struct TurnRecord { turn_id, timestamp, project, session_id, prompt_length, ... }
pub enum ContentType { Code, Prose, Markdown, Mixed }
pub fn estimate_tokens(text: &str) -> usize;
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()>;
pub fn append_jsonl<T: Serialize>(path: &Path, record: &T) -> Result<()>;
pub fn read_jsonl<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>>;
```

## Reference
- /private/tmp/attnroute/attnroute/telemetry_lib.py
- /private/tmp/attnroute/attnroute/telemetry_record.py
- /private/tmp/attnroute/attnroute/telemetry_report.py

---

# Task 3: attentive-core — attention engine (8-phase pipeline)
type: task
priority: 1
parent: attentive-qyj
labels: implementation
deps: attentive-telemetry

## Description
Port context_router.py. The core Router struct with 8-phase update_attention pipeline: decay → keyword activation → learned boost → co-activation (petgraph BFS 2-hop) → pinned floor → demoted penalty → predictive pre-warm → cache stability sort. Plus build_context_output (HOT full content, WARM TOC headers, COLD evicted). Config loading from keywords.json + router_overrides.json.

## Test
- Decay reduces all scores by category rate
- Keyword match sets score to 1.0
- Co-activation boosts related files by 0.35, transitive by 0.15
- Pinned files never drop below WARM threshold
- Max 3 HOT, 5 WARM enforced
- Max 20K chars enforced
- Cache stability: pinned first, then by streak, then by score
- Notification stripping (<task-notification>, <system-reminder>)
- State round-trip compatible with Python attn_state.json

## Key types
```rust
pub enum Tier { Hot, Warm, Cold }
pub struct Config { hot_threshold, warm_threshold, decay_rates, keyword_boost, ... }
pub struct AttentionState { scores, consecutive_turns, turn_count, last_update }
pub struct Router { config, compiled_keywords, coactivation_graph, ... }
pub struct RoutingResult { state, directly_activated, output, stats }
```

## Reference
- /private/tmp/attnroute/attnroute/context_router.py

---

# Task 4: attentive-plugins — trait + 3 builtins
type: task
priority: 1
parent: attentive-qyj
labels: implementation
deps: attentive-telemetry

## Description
Port plugins/base.py + loopbreaker.py + verifyfirst.py + burnrate.py. Plugin trait with on_session_start, on_prompt_pre, on_prompt_post, on_stop hooks. PluginRegistry loads enabled plugins from config.json. Each plugin persists state to JSON, events to JSONL.

LoopBreaker: tracks edit/write/bash attempts, detects 3+ similar signatures (0.7 similarity threshold), injects strategy-change context.
VerifyFirst: tracks files read vs written, detects edits on unread files, logs violations.
BurnRate: reads Claude Code stats-cache.json, calculates rolling burn rate, predicts quota exhaustion, warns at thresholds (30min, 10min).

## Test
- LoopBreaker detects loop after 3 similar attempts
- LoopBreaker doesn't false-positive on different files
- VerifyFirst flags write without prior read
- VerifyFirst allows write after read
- BurnRate calculates correct tokens/min from samples
- BurnRate warns at threshold
- Plugin state persists and reloads
- Plugin enable/disable via config.json

## Reference
- /private/tmp/attnroute/attnroute/plugins/base.py
- /private/tmp/attnroute/attnroute/plugins/loopbreaker.py
- /private/tmp/attnroute/attnroute/plugins/verifyfirst.py
- /private/tmp/attnroute/attnroute/plugins/burnrate.py

---

# Task 5: attentive-learn — learner + predictor
type: task
priority: 1
parent: attentive-qyj
labels: implementation
deps: attentive-telemetry

## Description
Port learner.py + predictor.py + oracle.py.

Learner: prompt-file affinity (IDF-weighted), co-activation discovery (Jaccard >= 0.25), per-file decay rhythms, session memory warm-start, usefulness scoring. Maturity levels: observing (0-24 turns, no boost) → active (25+, 0.35 weight). Association decay 0.995 per learn cycle.

Predictor: dual-mode (confident: file mentions + strong keywords + Markov sequences; fallback: recency + co-occurrence + popularity). Model persisted as JSON (not pickle like Python).

Oracle: task type classification (refactor, bug_fix, feature, review, exploration, config), token cost prediction from history.

## Test
- Learner observing mode applies zero boost
- Learner active mode applies 0.35-weighted boost
- IDF dampening reduces common-word boost
- Co-activation Jaccard threshold 0.25
- File rhythm learns from revisit gaps
- Session warmup loads previous scores
- Predictor confident mode finds file mentions in prompt
- Predictor fallback mode uses recency
- Model serializes to JSON and round-trips
- Oracle classifies task types correctly

## Reference
- /private/tmp/attnroute/attnroute/learner.py
- /private/tmp/attnroute/attnroute/predictor.py
- /private/tmp/attnroute/attnroute/oracle.py

---

# Task 6: attentive-index — BM25 + SQLite search
type: task
priority: 2
parent: attentive-qyj
labels: implementation
deps: attentive-telemetry

## Description
Port indexer.py. Hand-rolled BM25 (k1=1.5, b=0.75) over SQLite-stored documents. FTS5 for full-text search. Optional ONNX embeddings behind `embeddings` feature flag for semantic reranking (fusion: 0.6*bm25 + 0.4*cosine). Fallback SimpleTfIdf when BM25 index empty. Incremental updates via mtime checking.

## Test
- BM25 scores relevant docs higher than irrelevant
- BM25 handles empty corpus gracefully
- SQLite FTS5 full-text search returns matches
- Incremental update only reindexes changed files
- Query returns top-k sorted by relevance
- Fusion scoring with mock embeddings

## Reference
- /private/tmp/attnroute/attnroute/indexer.py

---

# Task 7: attentive-repo — tree-sitter + PageRank
type: task
priority: 2
parent: attentive-qyj
labels: implementation
deps: attentive-telemetry

## Description
Port repo_map.py + outliner.py. Feature-gated behind `tree-sitter`. Tree-sitter AST parsing for Python, JS/TS, Go, Rust, Java, C/C++. Extract symbols (functions, classes, methods, imports). Build dependency graph from imports. PageRank via petgraph. Token-budgeted output (~50 tokens/file). Regex fallback when tree-sitter unavailable.

## Test
- Parse Python file extracts functions and classes
- Parse JS file extracts functions and imports
- Dependency graph has correct edges
- PageRank ranks imported files higher
- Token budget respected
- Regex fallback produces reasonable output
- Outline extraction matches Python outliner output

## Reference
- /private/tmp/attnroute/attnroute/repo_map.py
- /private/tmp/attnroute/attnroute/outliner.py

---

# Task 8: attentive-compress — memory compression
type: task
priority: 3
parent: attentive-qyj
labels: implementation
deps: attentive-telemetry

## Description
Port compressor.py. Feature-gated behind `claude-api`. SQLite storage for compressed observations. 3-layer progressive retrieval (index → timeline → full). Claude API compression via reqwest + tokio. Fallback extractive compression without API. FTS5 search over observations.

## Test
- Fallback compression extracts key sentences
- Observation round-trips through SQLite
- Progressive retriever returns correct layers
- FTS5 search finds relevant observations

## Reference
- /private/tmp/attnroute/attnroute/compressor.py

---

# Task 9: attentive binary — CLI + hook entry
type: task
priority: 2
parent: attentive-qyj
labels: implementation
deps: attentive-core, attentive-plugins, attentive-learn, attentive-index

## Description
Port cli.py + installer.py. clap-based CLI with subcommands: init, status, report, diagnostic, benchmark, compress, graph, history, plugins, version, ingest (new). Hook entry point: reads stdin JSON, runs Router, outputs to stdout. Init command generates keywords.json from .claude/ directory scan. Ingest command scans Claude Code session JSONL to bootstrap learner + predictor.

## Test
- `attentive init` creates keywords.json
- `attentive status` shows config summary
- `attentive version` prints version
- Hook reads stdin JSON and produces output
- `attentive ingest` reads session JSONL files

## Reference
- /private/tmp/attnroute/attnroute/cli.py
- /private/tmp/attnroute/attnroute/installer.py

---

# Task 10: Integration tests + compatibility
type: task
priority: 2
parent: attentive-qyj
labels: testing
deps: attentive-core, attentive-learn, attentive-plugins

## Description
End-to-end integration tests. Full routing pipeline from cold start through 5 turns. Python state file compatibility (load Python-generated attn_state.json, learned_state.json, keywords.json into Rust structs). Criterion benchmarks targeting p99 < 50ms.

## Test
- Cold start produces valid output
- 5-turn sequence shows decay + keyword activation working
- Python attn_state.json loads into Rust AttentionState
- Python learned_state.json loads into Rust LearnedState
- Python keywords.json loads into Rust Config
- Benchmark: full routing turn p99 < 50ms

---

# Task 11: Git worktree support
type: task
priority: 2
parent: attentive-qyj
labels: implementation
deps: attentive-telemetry

## Description
In Paths, detect git worktrees via `git rev-parse --git-common-dir`. Place learned_state.json and predictor_model.json in common git dir (shared across worktrees). attn_state.json stays per-worktree (session-local). Keywords.json resolved per-worktree first, falls back to common.

## Test
- Detects worktree and resolves common dir
- Non-worktree falls back to normal paths
- Learned state path points to common dir in worktree
- Attn state path points to worktree-local dir

## Reference
Reddit feedback: jonieater's comment about git worktrees
