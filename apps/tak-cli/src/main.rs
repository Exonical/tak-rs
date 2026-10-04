//! `tak` — the TAK-RS command-line tool.
//!
//! Phase 1 ships the CoT codec commands (`tak cot decode|encode|validate`).
//! Connectivity commands (`tak connect`, `tak contacts`, …) arrive with
//! `tak-transport` / `tak-state`.

#![allow(clippy::doc_markdown)]

mod cot;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// TAK-RS command-line tool.
#[derive(Debug, Parser)]
#[command(name = "tak", version, about, propagate_version = true)]
struct Cli {
    /// Log filter (e.g. `debug`, `tak_cot=trace`). Also read from `TAK_LOG`.
    #[arg(long, global = true, env = "TAK_LOG", default_value = "warn")]
    log: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Cursor-on-Target XML utilities.
    #[command(subcommand)]
    Cot(cot::CotCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(&cli.log);
    let result = match cli.command {
        Command::Cot(cmd) => cot::run(cmd),
    };
    match result {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn init_logging(filter: &str) {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();
}
