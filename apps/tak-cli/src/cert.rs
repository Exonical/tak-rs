//! `tak cert …` subcommands.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context;
use clap::{Args, Subcommand};
use tak_crypto::{CertInfo, ClientIdentity, TrustStore};

/// Certificate utilities.
#[derive(Debug, Subcommand)]
pub(crate) enum CertCommand {
    /// Print subject, issuer, validity, SANs and fingerprint of certificates
    /// in a PEM bundle or PKCS#12 file (never prints key material).
    Inspect(InspectArgs),
}

/// Arguments for `tak cert inspect`.
#[derive(Debug, Args)]
pub(crate) struct InspectArgs {
    /// PEM bundle or PKCS#12 (`.p12`/`.pfx`) file.
    file: PathBuf,
    /// PKCS#12 password. Also read from `TAK_P12_PASSWORD`. TAK Server's
    /// default is `atakatak`.
    #[arg(long, env = "TAK_P12_PASSWORD", hide_env_values = true)]
    password: Option<String>,
    /// Emit JSON instead of text.
    #[arg(long)]
    json: bool,
}

pub(crate) fn run(cmd: CertCommand) -> anyhow::Result<ExitCode> {
    match cmd {
        CertCommand::Inspect(args) => inspect(&args),
    }
}

fn inspect(args: &InspectArgs) -> anyhow::Result<ExitCode> {
    let bytes =
        std::fs::read(&args.file).with_context(|| format!("reading {}", args.file.display()))?;
    let is_pem = bytes.windows(10).any(|w| w == b"-----BEGIN");
    let (infos, has_key) = if is_pem {
        (
            tak_crypto::inspect_pem(&bytes)?,
            bytes.windows(11).any(|w| w == b"PRIVATE KEY"),
        )
    } else {
        let password = args.password.clone().unwrap_or_default();
        match ClientIdentity::from_pkcs12(&bytes, &password) {
            Ok(identity) => (
                identity
                    .chain()
                    .iter()
                    .map(|c| tak_crypto::inspect_der(c))
                    .collect::<Result<Vec<_>, _>>()?,
                true,
            ),
            Err(tak_crypto::CryptoError::NoPrivateKey(_)) => {
                let mut trust = TrustStore::empty();
                trust.add_pkcs12(&bytes, &password)?;
                (trust.anchors().to_vec(), false)
            }
            Err(e) => return Err(e).context("reading PKCS#12 (wrong --password?)"),
        }
    };
    let now = time::OffsetDateTime::now_utc();
    if args.json {
        let items: Vec<_> = infos.iter().map(|i| to_json(i, now)).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "file": args.file.display().to_string(),
                "has_private_key": has_key,
                "certificates": items,
            }))?
        );
    } else {
        for (n, info) in infos.iter().enumerate() {
            if n > 0 {
                println!();
            }
            print_text(info, now);
        }
        println!(
            "private key: {}",
            if has_key { "present" } else { "absent" }
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn print_text(info: &CertInfo, now: time::OffsetDateTime) {
    println!("subject:      {}", info.subject);
    println!("issuer:       {}", info.issuer);
    println!("serial:       {}", info.serial);
    println!(
        "valid:        {} .. {} ({})",
        info.not_before,
        info.not_after,
        if info.is_valid_at(now) {
            "current"
        } else {
            "NOT VALID NOW"
        }
    );
    println!(
        "kind:         {}{}",
        if info.is_ca { "CA" } else { "end-entity" },
        if info.self_signed {
            ", self-signed"
        } else {
            ""
        }
    );
    if !info.subject_alt_names.is_empty() {
        println!("SAN:          {}", info.subject_alt_names.join(", "));
    }
    println!("key alg:      {}", info.public_key_algorithm);
    println!("sha256:       {}", info.sha256_fingerprint);
}

fn to_json(info: &CertInfo, now: time::OffsetDateTime) -> serde_json::Value {
    serde_json::json!({
        "subject": info.subject,
        "common_name": info.common_name,
        "issuer": info.issuer,
        "serial": info.serial,
        "not_before": info.not_before.to_string(),
        "not_after": info.not_after.to_string(),
        "valid_now": info.is_valid_at(now),
        "is_ca": info.is_ca,
        "self_signed": info.self_signed,
        "subject_alt_names": info.subject_alt_names,
        "public_key_algorithm": info.public_key_algorithm,
        "sha256": info.sha256_fingerprint,
    })
}
