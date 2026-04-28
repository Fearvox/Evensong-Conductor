use anyhow::Result;
use clap::{Parser, Subcommand};
use conductor_core::{config::ConductorConfig, console, ledger};
use std::net::SocketAddr;

#[derive(Debug, Parser)]
#[command(name = "conductor-core")]
#[command(about = "Evensong-Conductor core utilities")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    LedgerHealth,
    ServeConsole {
        #[arg(long, default_value = "127.0.0.1:4317")]
        bind: SocketAddr,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::LedgerHealth => {
            let config = ConductorConfig::from_env()?;
            let pool = ledger::connect(&config).await?;
            let event_id = ledger::write_health_event(&pool).await?;
            println!("ledger health event written: {event_id}");
        }
        Command::ServeConsole { bind } => {
            let config = ConductorConfig::from_env()?;
            let pool = ledger::connect(&config).await?;
            console::serve(pool, bind).await?;
        }
    }

    Ok(())
}
