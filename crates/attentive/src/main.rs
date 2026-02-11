mod cli;
mod commands;

use clap::Parser;
use cli::{Cli, Commands, PluginAction};

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => commands::init::run(),
        Commands::Ingest { file } => commands::ingest::run(file.as_deref()),
        Commands::Status { session } => commands::status::run(session.as_deref()),
        Commands::Version => commands::version::run(),
        Commands::HookUserPromptSubmit => commands::hooks::hook_user_prompt_submit(),
        Commands::HookSessionStart => commands::hooks::hook_session_start(),
        Commands::HookStop => commands::hooks::hook_stop(),
        Commands::Report => commands::report::run(),
        Commands::Diagnostic => commands::diagnostic::run(),
        Commands::Benchmark => commands::benchmark::run(),
        Commands::Compress => commands::compress::run(),
        Commands::Graph => commands::graph::run(),
        Commands::History { stats } => commands::history::run(stats),
        Commands::Plugins { action } => match action {
            Some(PluginAction::List) | None => commands::plugins::run_list(),
            Some(PluginAction::Enable { name }) => commands::plugins::run_enable(&name),
            Some(PluginAction::Disable { name }) => commands::plugins::run_disable(&name),
        },
    }
}
