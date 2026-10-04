//! `tak` — the TAK-RS command-line tool.
//!
//! * `tak cot decode|encode|validate` — CoT XML codec utilities.
//! * `tak connect|contacts|status` — live TAK Server sessions (TCP or mTLS).
//! * `tak cert inspect` — certificate / PKCS#12 inspection.

#![allow(clippy::doc_markdown)]

mod cert;
mod connect;
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
    /// Connect to a TAK server and stream events to stdout.
    Connect(Box<connect::ConnectArgs>),
    /// Connect, listen for a while, and list the contacts seen.
    Contacts(Box<connect::ContactsArgs>),
    /// Connect and report negotiated protocol and traffic counters.
    Status(Box<connect::StatusArgs>),
    /// Certificate and PKCS#12 utilities.
    #[command(subcommand)]
    Cert(cert::CertCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(&cli.log);
    let result = match cli.command {
        Command::Cot(cmd) => cot::run(cmd),
        Command::Connect(args) => connect::run_connect(*args),
        Command::Contacts(args) => connect::run_contacts(*args),
        Command::Status(args) => connect::run_status(*args),
        Command::Cert(cmd) => cert::run(cmd),
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
