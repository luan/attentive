use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "attentive")]
#[command(version)]
#[command(about = "Context routing for AI coding assistants")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize attentive in current project
    Init,

    /// Ingest Claude Code sessions to bootstrap learner
    Ingest {
        /// Path to session JSONL (auto-discovers if omitted)
        #[arg(short, long)]
        file: Option<String>,
    },

    /// Show configuration status
    Status,

    /// Print version information
    Version,

    /// Hook: Process user prompt (stdin/stdout JSON)
    #[command(name = "hook:user-prompt-submit")]
    HookUserPromptSubmit,

    /// Hook: Session start initialization
    #[command(name = "hook:session-start")]
    HookSessionStart,

    /// Hook: Record turn after Claude stops
    #[command(name = "hook:stop")]
    HookStop,

    // Stubs for future implementation
    /// Generate token usage report
    Report,

    /// Run diagnostic checks
    Diagnostic,

    /// Run performance benchmarks
    Benchmark,

    /// Compress observations
    Compress,

    /// Analyze dependency graph
    Graph,

    /// View turn history
    History {
        /// Show statistics summary
        #[arg(long)]
        stats: bool,
    },

    /// Manage plugins
    Plugins {
        #[command(subcommand)]
        action: Option<PluginAction>,
    },
}

#[derive(Subcommand)]
pub enum PluginAction {
    /// List all plugins
    List,
    /// Enable a plugin
    Enable { name: String },
    /// Disable a plugin
    Disable { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_version() {
        let cli = Cli::try_parse_from(["attentive", "version"]);
        assert!(cli.is_ok());
        assert!(matches!(cli.unwrap().command, Commands::Version));
    }

    #[test]
    fn test_cli_parse_init() {
        let cli = Cli::try_parse_from(["attentive", "init"]);
        assert!(cli.is_ok());
        assert!(matches!(cli.unwrap().command, Commands::Init));
    }

    #[test]
    fn test_cli_parse_ingest() {
        let cli = Cli::try_parse_from(["attentive", "ingest", "--file", "test.jsonl"]);
        assert!(cli.is_ok());
        if let Commands::Ingest { file } = cli.unwrap().command {
            assert_eq!(file, Some("test.jsonl".to_string()));
        } else {
            panic!("Expected Ingest command");
        }
    }

    #[test]
    fn test_cli_parse_hook_commands() {
        let hooks = ["hook:user-prompt-submit", "hook:session-start", "hook:stop"];

        for hook in hooks {
            let cli = Cli::try_parse_from(["attentive", hook]);
            assert!(cli.is_ok(), "Failed to parse {}", hook);
        }
    }
}
