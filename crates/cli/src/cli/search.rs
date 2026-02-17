use anyhow::Result;
use clap::{Args, Subcommand};

use localgpt_core::agent::tools::web_search::SearchRouter;
use localgpt_core::config::Config;

#[derive(Args)]
pub struct SearchArgs {
    #[command(subcommand)]
    pub command: SearchCommands,
}

#[derive(Subcommand)]
pub enum SearchCommands {
    /// Test web search with a query
    Test {
        /// The search query to test
        query: String,
    },
}

pub async fn run(args: SearchArgs) -> Result<()> {
    match args.command {
        SearchCommands::Test { query } => run_test(&query).await,
    }
}

async fn run_test(query: &str) -> Result<()> {
    let config = Config::load()?;

    let ws_config = config
        .tools
        .web_search
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No [tools.web_search] configured in config.toml"))?;

    let router = SearchRouter::from_config(ws_config)?;

    eprintln!("Searching with provider: {} ...", router.provider_name());

    let response = router.search(query).await?;

    println!(
        "OK: {} results in {}ms (cost: ${:.3})",
        response.meta.result_count, response.meta.latency_ms, response.meta.estimated_cost_usd
    );
    println!();

    for (i, result) in response.results.iter().enumerate() {
        println!("{}. {}", i + 1, result.title);
        println!("   {}", result.url);
        if !result.snippet.is_empty() {
            println!("   {}", result.snippet);
        }
        println!();
    }

    if response.results.is_empty() {
        println!("No results found.");
    }

    Ok(())
}
