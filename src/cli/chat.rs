use anyhow::Result;
use clap::Args;
use futures::StreamExt;
use std::io::{self, BufRead, Write};

use localgpt::agent::{Agent, AgentConfig};
use localgpt::config::Config;
use localgpt::memory::MemoryManager;

#[derive(Args)]
pub struct ChatArgs {
    /// Model to use (overrides config)
    #[arg(short, long)]
    pub model: Option<String>,

    /// Session ID to resume
    #[arg(short, long)]
    pub session: Option<String>,
}

pub async fn run(args: ChatArgs) -> Result<()> {
    let config = Config::load()?;
    let memory = MemoryManager::new(&config.memory)?;

    let agent_config = AgentConfig {
        model: args.model.unwrap_or(config.agent.default_model.clone()),
        context_window: config.agent.context_window,
        reserve_tokens: config.agent.reserve_tokens,
    };

    let mut agent = Agent::new(agent_config, &config, memory).await?;

    // Resume or create session
    if let Some(session_id) = args.session {
        agent.resume_session(&session_id).await?;
    } else {
        agent.new_session().await?;
    }

    println!(
        "LocalGPT v{} | Model: {} | Memory: {} chunks indexed\n",
        env!("CARGO_PKG_VERSION"),
        agent.model(),
        agent.memory_chunk_count()
    );
    println!("Type /help for commands, /quit to exit\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("You: ");
        stdout.flush()?;

        let mut input = String::new();
        if stdin.lock().read_line(&mut input)? == 0 {
            break; // EOF
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        // Handle commands
        if input.starts_with('/') {
            match handle_command(input, &mut agent).await {
                CommandResult::Continue => continue,
                CommandResult::Quit => break,
                CommandResult::Error(e) => {
                    eprintln!("Error: {}", e);
                    continue;
                }
            }
        }

        // Send message to agent with streaming
        print!("\nLocalGPT: ");
        stdout.flush()?;

        match agent.chat_stream(input).await {
            Ok(mut stream) => {
                let mut full_response = String::new();

                while let Some(result) = stream.next().await {
                    match result {
                        Ok(chunk) => {
                            print!("{}", chunk.delta);
                            stdout.flush()?;
                            full_response.push_str(&chunk.delta);
                        }
                        Err(e) => {
                            eprintln!("\nStream error: {}", e);
                            break;
                        }
                    }
                }

                // Add the response to session history
                agent.finish_chat_stream(&full_response);
                println!("\n");
            }
            Err(e) => {
                eprintln!("Error: {}\n", e);
            }
        }
    }

    println!("Goodbye!");
    Ok(())
}

enum CommandResult {
    Continue,
    Quit,
    Error(String),
}

async fn handle_command(input: &str, agent: &mut Agent) -> CommandResult {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let cmd = parts[0];

    match cmd {
        "/quit" | "/exit" | "/q" => CommandResult::Quit,

        "/help" | "/h" | "/?" => {
            println!("\nCommands:");
            println!("  /help, /h, /?     - Show this help");
            println!("  /quit, /exit, /q  - Exit chat");
            println!("  /new              - Start a fresh session (reloads memory context)");
            println!("  /compact          - Compact session history");
            println!("  /clear            - Clear session history (keeps context)");
            println!("  /memory <query>   - Search memory");
            println!("  /save             - Save current session");
            println!("  /status           - Show session status");
            println!();
            CommandResult::Continue
        }

        "/compact" => match agent.compact_session().await {
            Ok((before, after)) => {
                println!("\nSession compacted. Token count: {} → {}\n", before, after);
                CommandResult::Continue
            }
            Err(e) => CommandResult::Error(format!("Failed to compact: {}", e)),
        },

        "/clear" => {
            agent.clear_session();
            println!("\nSession cleared.\n");
            CommandResult::Continue
        }

        "/new" => match agent.new_session().await {
            Ok(()) => {
                println!("\nNew session started. Memory context reloaded.\n");
                CommandResult::Continue
            }
            Err(e) => CommandResult::Error(format!("Failed to create new session: {}", e)),
        }

        "/memory" => {
            if parts.len() < 2 {
                return CommandResult::Error("Usage: /memory <query>".into());
            }
            let query = parts[1..].join(" ");
            match agent.search_memory(&query).await {
                Ok(results) => {
                    println!("\nMemory search results for '{}':", query);
                    for (i, result) in results.iter().enumerate() {
                        println!(
                            "{}. [{}:{}] {}",
                            i + 1,
                            result.file,
                            result.line_start,
                            result.content.chars().take(100).collect::<String>()
                        );
                    }
                    println!();
                    CommandResult::Continue
                }
                Err(e) => CommandResult::Error(format!("Memory search failed: {}", e)),
            }
        }

        "/save" => match agent.save_session().await {
            Ok(path) => {
                println!("\nSession saved to: {}\n", path.display());
                CommandResult::Continue
            }
            Err(e) => CommandResult::Error(format!("Failed to save session: {}", e)),
        },

        "/status" => {
            let status = agent.session_status();
            println!("\nSession Status:");
            println!("  ID: {}", status.id);
            println!("  Messages: {}", status.message_count);
            println!("  Tokens: ~{}", status.token_count);
            println!("  Compactions: {}", status.compaction_count);
            println!();
            CommandResult::Continue
        }

        _ => CommandResult::Error(format!(
            "Unknown command: {}. Type /help for commands.",
            cmd
        )),
    }
}
