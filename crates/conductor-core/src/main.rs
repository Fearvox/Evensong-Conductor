use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use conductor_core::{config::ConductorConfig, console, hermes, ledger};
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
    HermesHealth {
        #[arg(long)]
        ssh_target: Option<String>,
        #[arg(long)]
        tmux_target: Option<String>,
        #[arg(long, default_value_t = 40)]
        capture_lines: usize,
        #[arg(long)]
        smoke: bool,
        #[arg(long)]
        smoke_message: Option<String>,
        #[arg(long)]
        smoke_expected: Option<String>,
        #[arg(long)]
        write_event: bool,
    },
    ServeConsole {
        #[arg(long, default_value = "127.0.0.1:4317")]
        bind: SocketAddr,
        #[arg(long)]
        allow_nonlocal: bool,
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
        Command::HermesHealth {
            ssh_target,
            tmux_target,
            capture_lines,
            smoke,
            smoke_message,
            smoke_expected,
            write_event,
        } => {
            let probe_config = hermes::HermesProbeConfig::from_inputs(
                ssh_target,
                tmux_target,
                capture_lines,
                smoke,
                smoke_message,
                smoke_expected,
            )?;
            let report = hermes::probe(&probe_config)?;

            println!(
                "{}",
                serde_json::to_string_pretty(&report.to_redacted_payload())?
            );

            if write_event {
                let config = ConductorConfig::from_env()?;
                let pool = ledger::connect(&config).await?;
                let event_id = ledger::write_hermes_health_event(&pool, &report).await?;
                println!("hermes health event written: {event_id}");
            }
        }
        Command::ServeConsole {
            bind,
            allow_nonlocal,
        } => {
            ensure_loopback_bind(bind, allow_nonlocal)?;

            if allow_nonlocal && !bind.ip().is_loopback() {
                eprintln!(
                    "warning: conductor console has no auth and is bound to non-loopback address {bind}"
                );
            }

            let config = ConductorConfig::from_env()?;
            let pool = ledger::connect(&config).await?;
            console::serve(pool, bind).await?;
        }
    }

    Ok(())
}

fn ensure_loopback_bind(bind: SocketAddr, allow_nonlocal: bool) -> Result<()> {
    if bind.ip().is_loopback() || allow_nonlocal {
        return Ok(());
    }

    bail!(
        "refusing to bind unauthenticated console to non-loopback address {bind}; use --allow-nonlocal to override"
    )
}

#[cfg(test)]
mod tests {
    use super::ensure_loopback_bind;

    #[test]
    fn allows_loopback_bind_by_default() {
        let bind = "127.0.0.1:4317".parse().expect("valid bind address");

        assert!(ensure_loopback_bind(bind, false).is_ok());
    }

    #[test]
    fn rejects_non_loopback_bind_by_default() {
        let bind = "0.0.0.0:4317".parse().expect("valid bind address");
        let error = ensure_loopback_bind(bind, false).expect_err("non-loopback should fail");

        assert!(error.to_string().contains("--allow-nonlocal"));
    }

    #[test]
    fn allows_non_loopback_bind_when_explicit() {
        let bind = "0.0.0.0:4317".parse().expect("valid bind address");

        assert!(ensure_loopback_bind(bind, true).is_ok());
    }
}
