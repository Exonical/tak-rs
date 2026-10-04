//! `tak connect`, `tak contacts`, `tak status`.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Args, ValueEnum};
use tak_core::{ObjectSource, TakEvent, TakUid, Timestamp, TransportId, WireEncoding};
use tak_cot::CotEvent;
use tak_crypto::{ClientIdentity, TlsOptions, TrustStore, Verification};
use tak_network::{Connector, Endpoint, Protocol, TcpConnector, TransportConfig};
use tak_state::Store;
use tak_transport::{Supervisor, SupervisorEvent, TlsConnector, WireMode};
use tokio_util::sync::CancellationToken;

/// Where and how to connect. Shared by all connection commands.
#[derive(Debug, Args)]
pub(crate) struct ServerArgs {
    /// Server as `host:port[:proto]`; proto is `tcp` (8087) or `ssl` (8089).
    /// Defaults to `ssl` when a certificate is given, else `tcp`.
    #[arg(long, short, env = "TAK_SERVER")]
    server: String,
    /// Client certificate chain (PEM). Requires `--key`.
    #[arg(long, requires = "key", conflicts_with = "p12")]
    cert: Option<PathBuf>,
    /// Client private key (PEM).
    #[arg(long, requires = "cert")]
    key: Option<PathBuf>,
    /// Client identity as PKCS#12 (TAK `user.p12`).
    #[arg(long)]
    p12: Option<PathBuf>,
    /// Password for `--p12` / `--truststore`. Also `TAK_P12_PASSWORD`.
    #[arg(
        long,
        env = "TAK_P12_PASSWORD",
        hide_env_values = true,
        default_value = "atakatak"
    )]
    p12_password: String,
    /// CA bundle (PEM) to trust. Repeatable.
    #[arg(long)]
    ca: Vec<PathBuf>,
    /// Trust store as PKCS#12 (TAK `truststore-root.p12`). Repeatable.
    #[arg(long)]
    truststore: Vec<PathBuf>,
    /// Trust the system's web PKI roots in addition to `--ca`/`--truststore`.
    #[arg(long)]
    webpki_roots: bool,
    /// Verify the server chain but not its hostname (common with TAK
    /// servers reached by IP).
    #[arg(long)]
    no_verify_hostname: bool,
    /// Skip server certificate verification entirely. Debugging only.
    #[arg(long)]
    insecure: bool,
    /// Our UID in negotiation messages (default: random).
    #[arg(long)]
    uid: Option<String>,
    /// Never upgrade to TAK Protocol; stay on CoT XML.
    #[arg(long)]
    xml_only: bool,
    /// Connect timeout in seconds.
    #[arg(long, default_value_t = 10)]
    connect_timeout: u64,
}

/// Output format for streamed events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum StreamFormat {
    /// One line per event.
    Summary,
    /// One JSON object per line (domain model).
    Jsonl,
    /// Raw CoT XML, one document per line.
    Xml,
}

/// Arguments for `tak connect`.
#[derive(Debug, Args)]
pub(crate) struct ConnectArgs {
    #[command(flatten)]
    server: ServerArgs,
    /// Output format.
    #[arg(long, short, value_enum, default_value_t = StreamFormat::Summary)]
    format: StreamFormat,
    /// Stop after this many seconds (default: run until Ctrl-C).
    #[arg(long)]
    duration: Option<u64>,
    /// Send CoT XML read from this file (or `-` for stdin) once connected.
    #[arg(long)]
    send: Option<PathBuf>,
}

/// Arguments for `tak contacts`.
#[derive(Debug, Args)]
pub(crate) struct ContactsArgs {
    #[command(flatten)]
    server: ServerArgs,
    /// Listen this many seconds before printing.
    #[arg(long, default_value_t = 10)]
    r#for: u64,
    /// JSON output.
    #[arg(long)]
    json: bool,
}

/// Arguments for `tak status`.
#[derive(Debug, Args)]
pub(crate) struct StatusArgs {
    #[command(flatten)]
    server: ServerArgs,
    /// Observe this many seconds before printing.
    #[arg(long, default_value_t = 5)]
    r#for: u64,
    /// JSON output.
    #[arg(long)]
    json: bool,
}

struct Connection {
    handle: tak_transport::SupervisorHandle,
    cancel: CancellationToken,
    describe: String,
    uid: TakUid,
}

fn runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting async runtime")
}

fn client_uid(args: &ServerArgs) -> anyhow::Result<TakUid> {
    let uid = args
        .uid
        .clone()
        .unwrap_or_else(|| format!("TAK-RS-{}", uuid::Uuid::new_v4()));
    Ok(TakUid::new(uid)?)
}

fn transport_config(args: &ServerArgs) -> TransportConfig {
    TransportConfig {
        connect_timeout: Duration::from_secs(args.connect_timeout),
        ..TransportConfig::DEFAULT
    }
}

fn parse_server(args: &ServerArgs) -> anyhow::Result<Endpoint> {
    let has_identity = args.cert.is_some() || args.p12.is_some();
    let spec = if args.server.matches(':').count() == 1 {
        format!(
            "{}:{}",
            args.server,
            if has_identity { "ssl" } else { "tcp" }
        )
    } else {
        args.server.clone()
    };
    Ok(spec.parse::<Endpoint>()?)
}

fn identity(args: &ServerArgs) -> anyhow::Result<Option<ClientIdentity>> {
    if let Some(p12) = &args.p12 {
        return Ok(Some(
            ClientIdentity::from_pkcs12_file(p12, &args.p12_password)
                .with_context(|| format!("loading {}", p12.display()))?,
        ));
    }
    if let (Some(cert), Some(key)) = (&args.cert, &args.key) {
        return Ok(Some(
            ClientIdentity::from_pem_files(cert, key).context("loading --cert/--key")?,
        ));
    }
    Ok(None)
}

fn trust(args: &ServerArgs) -> anyhow::Result<TrustStore> {
    let mut trust = if args.webpki_roots {
        TrustStore::webpki_roots()
    } else {
        TrustStore::empty()
    };
    for ca in &args.ca {
        trust
            .add_pem_file(ca)
            .with_context(|| format!("loading {}", ca.display()))?;
    }
    for ts in &args.truststore {
        trust
            .add_pkcs12_file(ts, &args.p12_password)
            .with_context(|| format!("loading {}", ts.display()))?;
    }
    Ok(trust)
}

fn connector(args: &ServerArgs) -> anyhow::Result<(Arc<dyn Connector>, String)> {
    let endpoint = parse_server(args)?;
    let addr = endpoint
        .socket_addr_string()
        .ok_or_else(|| anyhow::anyhow!("`{}` is not a directly reachable endpoint", args.server))?;
    let port = endpoint
        .port
        .ok_or_else(|| anyhow::anyhow!("`{}` has no port", args.server))?;
    let cfg = transport_config(args);
    match endpoint.protocol {
        Protocol::Tcp => {
            if args.cert.is_some() || args.p12.is_some() {
                tracing::warn!("certificate given but protocol is tcp; it will not be used");
            }
            let c = TcpConnector::new(addr, cfg);
            let d = c.describe();
            Ok((Arc::new(c), d))
        }
        Protocol::Tls => {
            let verification = if args.insecure {
                Verification::DangerousNoVerify
            } else if args.no_verify_hostname {
                Verification::TrustedChainAnyName
            } else {
                Verification::Full
            };
            let trust = trust(args)?;
            if trust.is_empty() && verification != Verification::DangerousNoVerify {
                bail!(
                    "ssl endpoint needs a trust anchor: pass --ca / --truststore (or --webpki-roots), or --insecure for debugging"
                );
            }
            let tls = tak_crypto::build_client_config(TlsOptions {
                trust,
                identity: identity(args)?,
                verification,
            })?;
            let c = TlsConnector::new(endpoint.host.clone(), port, tls, cfg)?;
            let d = c.describe();
            Ok((Arc::new(c), d))
        }
        other => bail!("protocol `{other}` is not supported yet (use tcp or ssl)"),
    }
}

fn open(args: &ServerArgs) -> anyhow::Result<Connection> {
    let (connector, describe) = connector(args)?;
    let uid = client_uid(args)?;
    let mut sup = Supervisor::new(connector, uid.clone());
    sup.xml_only = args.xml_only;
    let cancel = CancellationToken::new();
    let handle = sup.spawn(cancel.clone());
    Ok(Connection {
        handle,
        cancel,
        describe,
        uid,
    })
}

async fn stop_signal(cancel: CancellationToken, duration: Option<u64>) {
    let ctrl_c = tokio::signal::ctrl_c();
    match duration {
        Some(secs) => {
            tokio::select! {
                _ = ctrl_c => {}
                () = tokio::time::sleep(Duration::from_secs(secs)) => {}
            }
        }
        None => {
            let _ = ctrl_c.await;
        }
    }
    cancel.cancel();
}

pub(crate) fn run_connect(args: ConnectArgs) -> anyhow::Result<ExitCode> {
    let rt = runtime()?;
    rt.block_on(async move {
        let mut conn = open(&args.server)?;
        tokio::spawn(stop_signal(conn.cancel.clone(), args.duration));
        let to_send = match &args.send {
            Some(path) => Some(read_events(path)?),
            None => None,
        };
        let mut out = std::io::BufWriter::new(std::io::stdout().lock());
        let mut connected_once = false;
        while let Some(ev) = conn.handle.events.recv().await {
            match ev {
                SupervisorEvent::Connected { transport } => {
                    connected_once = true;
                    tracing::info!(%transport, "connected");
                    if let Some(events) = &to_send {
                        for e in events {
                            conn.handle.outbound.send(e.clone()).await.ok();
                        }
                    }
                }
                SupervisorEvent::WireMode(mode) => tracing::info!(?mode, "wire mode"),
                SupervisorEvent::Received { transport, event } => {
                    write_event(&mut out, args.format, &transport, &event)?;
                    out.flush()?;
                }
                SupervisorEvent::Disconnected {
                    reason, retry_in, ..
                } => {
                    eprintln!("disconnected: {reason}; retrying in {retry_in:.1?}");
                }
                SupervisorEvent::Stopped => break,
                _ => {}
            }
        }
        conn.handle.shutdown().await;
        Ok(if connected_once {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        })
    })
}

pub(crate) fn run_contacts(args: ContactsArgs) -> anyhow::Result<ExitCode> {
    let rt = runtime()?;
    rt.block_on(async move {
        let mut conn = open(&args.server)?;
        tokio::spawn(stop_signal(conn.cancel.clone(), Some(args.r#for)));
        let mut store = Store::default();
        let mut connected = false;
        while let Some(ev) = conn.handle.events.recv().await {
            match ev {
                SupervisorEvent::Connected { .. } => connected = true,
                SupervisorEvent::Received { transport, event } => {
                    if let Some(domain) = to_domain(&transport, &event) {
                        store.apply(domain);
                    }
                }
                SupervisorEvent::Stopped => break,
                _ => {}
            }
        }
        conn.handle.shutdown().await;
        let now = Timestamp::now();
        let mut contacts: Vec<_> = store.contacts().collect();
        contacts.sort_by(|a, b| a.callsign.as_str().cmp(b.callsign.as_str()));
        if args.json {
            println!("{}", serde_json::to_string_pretty(&contacts)?);
        } else {
            println!(
                "{:<20} {:<36} {:<12} {:>10} {:>11} {:<6} DEVICE",
                "CALLSIGN", "UID", "TEAM/ROLE", "LAT", "LON", "STALE"
            );
            for c in &contacts {
                let team = c
                    .team
                    .as_ref()
                    .map(|t| t.as_str().to_owned())
                    .unwrap_or_default();
                let role = c
                    .role
                    .as_ref()
                    .map(|r| format!("/{}", r.as_str()))
                    .unwrap_or_default();
                let device = c
                    .device
                    .as_ref()
                    .and_then(|d| d.platform.clone())
                    .unwrap_or_default();
                println!(
                    "{:<20} {:<36} {:<12} {:>10.5} {:>11.5} {:<6} {}",
                    c.callsign.as_str(),
                    c.uid.as_str(),
                    format!("{team}{role}"),
                    c.position.lat.degrees(),
                    c.position.lon.degrees(),
                    if c.is_stale_at(now) { "yes" } else { "no" },
                    device
                );
            }
            eprintln!(
                "{} contact(s) seen in {}s via {}",
                contacts.len(),
                args.r#for,
                conn.describe
            );
        }
        Ok(if connected {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        })
    })
}

pub(crate) fn run_status(args: StatusArgs) -> anyhow::Result<ExitCode> {
    let rt = runtime()?;
    rt.block_on(async move {
        let mut conn = open(&args.server)?;
        tokio::spawn(stop_signal(conn.cancel.clone(), Some(args.r#for)));
        let started = std::time::Instant::now();
        let mut transport = None;
        let mut mode = WireMode::CotXml;
        let mut received = 0u64;
        let mut disconnects = Vec::new();
        let mut store = Store::default();
        let mut connected_at = None;
        while let Some(ev) = conn.handle.events.recv().await {
            match ev {
                SupervisorEvent::Connected { transport: t } => {
                    connected_at = Some(started.elapsed());
                    transport = Some(t);
                }
                SupervisorEvent::WireMode(m) => mode = m,
                SupervisorEvent::Received { transport, event } => {
                    received += 1;
                    if let Some(domain) = to_domain(&transport, &event) {
                        store.apply(domain);
                    }
                }
                SupervisorEvent::Disconnected { reason, .. } => disconnects.push(reason),
                SupervisorEvent::Stopped => break,
                _ => {}
            }
        }
        conn.handle.shutdown().await;
        let connected = transport.is_some();
        let summary = serde_json::json!({
            "server": conn.describe,
            "client_uid": conn.uid.as_str(),
            "connected": connected,
            "connect_latency_ms": connected_at.map(|d| d.as_millis() as u64),
            "transport": transport.as_ref().map(|t| t.as_str().to_owned()),
            "wire_mode": match mode { WireMode::CotXml => "cot-xml", WireMode::TakProtocolV1 => "tak-protocol-v1" },
            "events_received": received,
            "contacts": store.contact_count(),
            "objects": store.object_count(),
            "disconnects": disconnects,
            "observed_seconds": args.r#for,
        });
        if args.json {
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else if let Some(obj) = summary.as_object() {
            for (k, v) in obj {
                let v = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                println!("{k:<20} {v}");
            }
        }
        Ok(if connected {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        })
    })
}

fn to_domain(transport: &TransportId, event: &CotEvent) -> Option<TakEvent> {
    let source = ObjectSource::remote(transport.clone(), WireEncoding::CotXml);
    match tak_cot::adapter::to_tak_event(event, source, Timestamp::now()) {
        Ok(e) => Some(e),
        Err(err) => {
            tracing::debug!(uid = %event.uid, %err, "event not representable in domain model");
            None
        }
    }
}

fn write_event(
    out: &mut impl std::io::Write,
    format: StreamFormat,
    transport: &TransportId,
    event: &CotEvent,
) -> anyhow::Result<()> {
    match format {
        StreamFormat::Summary => {
            let callsign = event
                .detail
                .get("contact")
                .and_then(|c| c.attr("callsign"))
                .unwrap_or("-");
            writeln!(
                out,
                "{} {:<24} {:<36} {:<12} {:>10.5} {:>11.5}",
                event.time.to_cot_string(),
                callsign,
                event.uid.as_str(),
                event.cot_type.as_str(),
                event.point.lat,
                event.point.lon
            )?;
        }
        StreamFormat::Jsonl => {
            let domain = to_domain(transport, event);
            writeln!(
                out,
                "{}",
                serde_json::json!({
                    "transport": transport.as_str(),
                    "uid": event.uid.as_str(),
                    "type": event.cot_type.as_str(),
                    "time": event.time.to_cot_string(),
                    "stale": event.stale.to_cot_string(),
                    "point": { "lat": event.point.lat, "lon": event.point.lon, "hae": event.point.hae },
                    "domain": domain,
                })
            )?;
        }
        StreamFormat::Xml => writeln!(out, "{}", tak_cot::to_xml(event)?)?,
    }
    Ok(())
}

fn read_events(path: &PathBuf) -> anyhow::Result<Vec<CotEvent>> {
    let bytes = if path.as_os_str() == "-" {
        let mut b = Vec::new();
        std::io::Read::read_to_end(&mut std::io::stdin(), &mut b)?;
        b
    } else {
        std::fs::read(path).with_context(|| format!("reading {}", path.display()))?
    };
    let text = String::from_utf8(bytes).context("--send file is not UTF-8")?;
    let mut decoder = tak_network::StreamDecoder::new(text.len().max(1));
    decoder.extend(text.as_bytes());
    let mut events = Vec::new();
    while let Some(frame) = decoder.next_frame()? {
        let xml = frame
            .as_xml_str()
            .ok_or_else(|| anyhow::anyhow!("--send only accepts CoT XML"))?;
        events.push(tak_cot::parse(xml)?);
    }
    if events.is_empty() {
        bail!("no CoT events in --send input");
    }
    Ok(events)
}
