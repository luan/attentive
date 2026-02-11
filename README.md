# attentive

Attention router for Claude Code.
Learns which files matter per prompt, injects them as context
via hooks so Claude starts each turn already focused.

## How it works

1. **Hooks** intercept Claude Code lifecycle events:
   - `user-prompt-submit` — routes attention, injects HOT/WARM files
   - `stop` — records which files were actually used, trains learner
   - `session-start` — dashboard, project switch detection

2. **Learner** builds word→file associations from session history
   (TF-IDF weighted). After enough data, it predicts which files
   you'll need before you ask.

3. **Router** scores every known file per prompt (7-phase pipeline:
   decay → co-activation → pinned floors → demoted penalty →
   learner boost → cache stability → truncation).

4. **Tiers** determine injection strategy:
   - **HOT** (≥0.8) — full file content
   - **WARM** (≥0.25) — table of contents (function signatures)
   - **COLD** (<0.25) — evicted

## Install

```
cargo install --path crates/attentive
```

## Setup

```bash
# Initialize config + hooks
attentive init

# Bootstrap learner from existing Claude Code sessions
attentive ingest
```

## Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize config and install Claude Code hooks |
| `ingest` | Bootstrap learner from session JSONL files |
| `benchmark` | Measure token reduction on current repo |
| `status` | Show config and learner state |
| `diagnostic` | Check dependencies and health |
| `history` | View turn history with filters |
| `report` | Generate token usage report |
| `compress` | Compress observations via Claude API |
| `graph` | Analyze file dependency graph |
| `plugins` | Manage plugins |

## Workspace crates

| Crate | Purpose |
|-------|---------|
| `attentive` | CLI binary and hook implementations |
| `attentive-core` | Router, attention state, config, tiers |
| `attentive-learn` | TF-IDF learner (word→file associations) |
| `attentive-telemetry` | Path resolution, JSONL I/O, turn records |
| `attentive-plugins` | Plugin system (burn rate, loop breaker, verify-first) |
| `attentive-index` | SQLite index with fastembed semantic search |
| `attentive-compress` | Claude API observation compression |
| `attentive-repo` | Git repo analysis |

## State files

All state is project-scoped under `~/.claude/projects/<project-hash>/`:

- `learned_state.json` — learner associations
- `attn_state.json` — current attention scores
- `session_state.json` — session metadata

Global config: `~/.claude/attentive.json`

## License

MIT
