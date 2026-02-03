use anyhow::Result;
use clap::Parser;

mod cli;

use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    match cli.command {
        Commands::Chat(args) => cli::chat::run(args, &cli.agent).await,
        Commands::Ask(args) => cli::ask::run(args, &cli.agent).await,
        Commands::Daemon(args) => cli::daemon::run(args, &cli.agent).await,
        Commands::Memory(args) => cli::memory::run(args, &cli.agent).await,
        Commands::Config(args) => cli::config::run(args).await,
    }
}
