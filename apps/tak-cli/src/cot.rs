//! `tak cot …` subcommands.

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, bail};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;
use tak_core::{
    Callsign, CotType, Extensions, How, ObjectSource, Role, TakUid, Team, Timestamp, TransportId,
    WireEncoding,
};
use tak_cot::adapter::{classify, to_tak_event};
use tak_cot::detail::{ContactDetail, DetailExt, GroupDetail};
use tak_cot::{CotEvent, CotPoint, UNKNOWN_SENTINEL, WriteOptions};
use tak_network::{Frame, StreamDecoder};

/// CoT XML codec commands.
#[derive(Debug, Subcommand)]
pub(crate) enum CotCommand {
    /// Parse CoT XML and print it in a readable form.
    Decode(DecodeArgs),
    /// Build a CoT event from command-line fields and print the XML.
    Encode(Box<EncodeArgs>),
    /// Check that files contain well-formed, valid CoT; exit 1 otherwise.
    Validate(ValidateArgs),
}

/// Output format for `decode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Format {
    /// One-line-per-field human summary.
    Summary,
    /// JSON (one object per event).
    Json,
    /// Re-serialised, indented XML.
    Xml,
}

/// Arguments for `tak cot decode`.
#[derive(Debug, Args)]
pub(crate) struct DecodeArgs {
    /// Input file (`-` or omitted for stdin). May contain several
    /// concatenated events, as captured from a TAK stream.
    #[arg(default_value = "-")]
    input: PathBuf,
    /// Output format.
    #[arg(long, short, value_enum, default_value_t = Format::Summary)]
    format: Format,
    /// Also convert to the TAK-RS domain model (`Contact`, `TakObject`, …) and
    /// include it in the output.
    #[arg(long)]
    domain: bool,
}

/// Arguments for `tak cot encode`.
#[derive(Debug, Args)]
pub(crate) struct EncodeArgs {
    /// Event UID (random UUID if omitted).
    #[arg(long)]
    uid: Option<String>,
    /// CoT type, e.g. `a-f-G-U-C`.
    #[arg(long, short = 't', default_value = "a-f-G-U-C")]
    r#type: String,
    /// `how` attribute, e.g. `m-g` or `h-e`.
    #[arg(long, default_value = "h-e")]
    how: String,
    /// Latitude in decimal degrees.
    #[arg(long, allow_negative_numbers = true)]
    lat: f64,
    /// Longitude in decimal degrees.
    #[arg(long, allow_negative_numbers = true)]
    lon: f64,
    /// Height above ellipsoid in metres.
    #[arg(long, allow_negative_numbers = true)]
    hae: Option<f64>,
    /// Circular error in metres.
    #[arg(long)]
    ce: Option<f64>,
    /// Linear error in metres.
    #[arg(long)]
    le: Option<f64>,
    /// Event time (RFC 3339); defaults to now.
    #[arg(long)]
    time: Option<String>,
    /// Seconds after `time` at which the event goes stale.
    #[arg(long, default_value_t = 300)]
    stale_secs: i64,
    /// Add a `<contact callsign="…"/>` detail.
    #[arg(long)]
    callsign: Option<String>,
    /// Add a `<__group name="…"/>` team colour (requires `--callsign`).
    #[arg(long, requires = "callsign")]
    team: Option<String>,
    /// Add a `<__group role="…"/>` role (requires `--team`).
    #[arg(long, requires = "team")]
    role: Option<String>,
    /// Add a `<remarks>` detail.
    #[arg(long)]
    remarks: Option<String>,
    /// Raw XML fragment(s) to append inside `<detail>` verbatim. Repeatable.
    #[arg(long = "detail", value_name = "XML")]
    details: Vec<String>,
    /// Indent the output instead of emitting a single wire line.
    #[arg(long)]
    pretty: bool,
}

/// Arguments for `tak cot validate`.
#[derive(Debug, Args)]
pub(crate) struct ValidateArgs {
    /// Files to check (`-` for stdin). Each may contain several events.
    #[arg(default_value = "-")]
    inputs: Vec<PathBuf>,
    /// Also require that the event maps onto the domain model and that
    /// `stale` is after `time`.
    #[arg(long)]
    strict: bool,
    /// Only print failures.
    #[arg(long, short)]
    quiet: bool,
}

/// Dispatch a `cot` subcommand.
pub(crate) fn run(cmd: CotCommand) -> anyhow::Result<ExitCode> {
    match cmd {
        CotCommand::Decode(a) => decode(&a),
        CotCommand::Encode(a) => encode(*a),
        CotCommand::Validate(a) => Ok(validate(&a)),
    }
}

fn read_input(path: &PathBuf) -> anyhow::Result<Vec<u8>> {
    if path.as_os_str() == "-" {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf).context("reading stdin")?;
        Ok(buf)
    } else {
        fs::read(path).with_context(|| format!("reading {}", path.display()))
    }
}

/// Split a capture into individual CoT documents using the stream framer, so
/// files produced by `tcpdump`/`nc` against a TAK stream work unchanged.
fn split_documents(bytes: &[u8]) -> anyhow::Result<Vec<String>> {
    let mut decoder = StreamDecoder::new(bytes.len().max(1));
    decoder.extend(bytes);
    let mut docs = Vec::new();
    while let Some(frame) = decoder.next_frame()? {
        match frame {
            Frame::CotXml(xml) => {
                docs.push(String::from_utf8(xml.to_vec()).context("input is not UTF-8")?);
            }
            Frame::TakProtobuf(_) => {
                bail!("input contains TAK Protocol (protobuf) frames; `tak cot` only handles XML")
            }
        }
    }
    if decoder.pending() > 0 {
        bail!(
            "trailing {} bytes do not form a complete <event>",
            decoder.pending()
        );
    }
    Ok(docs)
}

fn cli_source() -> ObjectSource {
    ObjectSource::remote(TransportId::new("cli"), WireEncoding::CotXml)
}

fn decode(args: &DecodeArgs) -> anyhow::Result<ExitCode> {
    let bytes = read_input(&args.input)?;
    let docs = split_documents(&bytes)?;
    if docs.is_empty() {
        bail!("no <event> documents found in input");
    }
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let now = Timestamp::now();
    for (i, doc) in docs.iter().enumerate() {
        let event = tak_cot::parse(doc).with_context(|| format!("event #{}", i + 1))?;
        match args.format {
            Format::Summary => {
                if i > 0 {
                    writeln!(out)?;
                }
                write_summary(&mut out, &event, args.domain, now)?;
            }
            Format::Json => {
                let mut value = event_json(&event);
                if args.domain {
                    let domain = to_tak_event(&event, cli_source(), now)?;
                    value["domain"] = serde_json::to_value(&domain)?;
                }
                serde_json::to_writer(&mut out, &value)?;
                writeln!(out)?;
            }
            Format::Xml => {
                writeln!(
                    out,
                    "{}",
                    tak_cot::to_xml_with(&event, &WriteOptions::PRETTY)?
                )?;
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn write_summary(
    out: &mut impl Write,
    ev: &CotEvent,
    domain: bool,
    now: Timestamp,
) -> anyhow::Result<()> {
    writeln!(out, "uid:      {}", ev.uid)?;
    writeln!(out, "type:     {} ({:?})", ev.cot_type, classify(ev))?;
    if let Some(how) = &ev.how {
        writeln!(out, "how:      {how}")?;
    }
    writeln!(out, "time:     {}", ev.time)?;
    writeln!(out, "start:    {}", ev.start)?;
    writeln!(
        out,
        "stale:    {}{}",
        ev.stale,
        if ev.is_stale_at(now) { "  (stale)" } else { "" }
    )?;
    let p = &ev.point;
    write!(out, "point:    {:.7}, {:.7}", p.lat, p.lon)?;
    match p.hae_metres() {
        Some(h) => write!(out, "  hae={h}m")?,
        None => write!(out, "  hae=?")?,
    }
    match (p.ce_metres(), p.le_metres()) {
        (Some(ce), Some(le)) => writeln!(out, "  ce={ce}m le={le}m")?,
        (Some(ce), None) => writeln!(out, "  ce={ce}m")?,
        (None, Some(le)) => writeln!(out, "  le={le}m")?,
        (None, None) => writeln!(out)?,
    }
    for (k, v) in &ev.extra_attributes {
        writeln!(out, "attr:     {k}={v}")?;
    }
    if let Ok(Some(c)) = ev.detail.typed::<ContactDetail>() {
        writeln!(out, "callsign: {}", c.callsign)?;
    }
    if let Ok(Some(g)) = ev.detail.typed::<GroupDetail>() {
        writeln!(
            out,
            "group:    {} / {}",
            if g.name.is_empty() { "-" } else { &g.name },
            if g.role.is_empty() { "-" } else { &g.role }
        )?;
    }
    let names: Vec<_> = ev.detail.iter().map(|n| n.name.as_str()).collect();
    writeln!(out, "detail:   [{}]", names.join(", "))?;
    if domain {
        let d = to_tak_event(ev, cli_source(), now)?;
        writeln!(out, "domain:   {d:#?}")?;
    }
    Ok(())
}

fn event_json(ev: &CotEvent) -> serde_json::Value {
    let p = &ev.point;
    json!({
        "version": ev.version,
        "uid": ev.uid.as_str(),
        "type": ev.cot_type.as_str(),
        "classification": format!("{:?}", classify(ev)),
        "how": ev.how.as_ref().map(ToString::to_string),
        "time": ev.time.to_string(),
        "start": ev.start.to_string(),
        "stale": ev.stale.to_string(),
        "access": ev.access,
        "opex": ev.opex,
        "qos": ev.qos,
        "caveat": ev.caveat,
        "releaseableTo": ev.releaseable_to,
        "attributes": ev
            .extra_attributes
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect::<serde_json::Map<_, _>>(),
        "point": {
            "lat": p.lat,
            "lon": p.lon,
            "hae": p.hae_metres(),
            "ce": p.ce_metres(),
            "le": p.le_metres(),
        },
        "detail": ev.detail.iter().collect::<Vec<_>>(),
    })
}

fn encode(args: EncodeArgs) -> anyhow::Result<ExitCode> {
    let uid = match args.uid {
        Some(u) => TakUid::new(u)?,
        None => TakUid::new(uuid::Uuid::new_v4().to_string())?,
    };
    let cot_type = CotType::new(args.r#type)?;
    let how = How::new(args.how)?;
    let time = match &args.time {
        Some(t) => Timestamp::parse_rfc3339(t)?,
        None => Timestamp::now(),
    };
    if args.stale_secs <= 0 {
        bail!("--stale-secs must be positive");
    }
    let stale = Timestamp::from_unix_millis(
        time.unix_millis()
            .checked_add(args.stale_secs.saturating_mul(1000))
            .context("stale time overflows")?,
    )?;
    let point = CotPoint {
        lat: args.lat,
        lon: args.lon,
        hae: args.hae.unwrap_or(UNKNOWN_SENTINEL),
        ce: args.ce.unwrap_or(UNKNOWN_SENTINEL),
        le: args.le.unwrap_or(UNKNOWN_SENTINEL),
    };
    point.validate()?;

    let mut detail = Extensions::default();
    if let Some(cs) = &args.callsign {
        let callsign = Callsign::new(cs)?;
        detail.set_typed(&ContactDetail {
            callsign: callsign.as_str().to_owned(),
            ..ContactDetail::default()
        });
        if let Some(team) = &args.team {
            let team = Team::parse(team)?;
            let role = match &args.role {
                Some(r) => Some(Role::parse(r)?),
                None => None,
            };
            detail.set_typed(&GroupDetail {
                name: team.as_str().to_owned(),
                role: role.map(|r| r.as_str().to_owned()).unwrap_or_default(),
                ..GroupDetail::default()
            });
        }
    }
    if let Some(r) = &args.remarks {
        detail.set_typed(&tak_cot::detail::RemarksDetail {
            text: r.clone(),
            ..tak_cot::detail::RemarksDetail::default()
        });
    }
    for fragment in &args.details {
        for node in parse_fragment(fragment)? {
            detail.push(node);
        }
    }

    let event = CotEvent::new(uid, cot_type, point, time, stale)
        .with_how(how)
        .with_detail(detail);
    let opts = if args.pretty {
        WriteOptions::PRETTY
    } else {
        WriteOptions::WIRE
    };
    println!("{}", tak_cot::to_xml_with(&event, &opts)?);
    Ok(ExitCode::SUCCESS)
}

/// Parse a raw `<detail>` fragment by wrapping it in a throwaway event.
fn parse_fragment(fragment: &str) -> anyhow::Result<Vec<tak_core::DetailNode>> {
    let wrapper = format!(
        r#"<event version="2.0" uid="fragment" type="t-x" time="1970-01-01T00:00:00Z" start="1970-01-01T00:00:00Z" stale="1970-01-01T00:00:00Z"><point lat="0" lon="0" hae="0" ce="0" le="0"/><detail>{fragment}</detail></event>"#
    );
    let ev = tak_cot::parse(&wrapper)
        .with_context(|| format!("invalid --detail fragment `{fragment}`"))?;
    Ok(ev.detail.into_nodes())
}

fn validate(args: &ValidateArgs) -> ExitCode {
    let mut failures = 0usize;
    let mut total = 0usize;
    let now = Timestamp::now();
    for path in &args.inputs {
        let label = if path.as_os_str() == "-" {
            "<stdin>".to_owned()
        } else {
            path.display().to_string()
        };
        let bytes = match read_input(path) {
            Ok(b) => b,
            Err(e) => {
                failures += 1;
                eprintln!("{label}: FAIL: {e:#}");
                continue;
            }
        };
        let docs = match split_documents(&bytes) {
            Ok(d) if d.is_empty() => {
                failures += 1;
                eprintln!("{label}: FAIL: no <event> found");
                continue;
            }
            Ok(d) => d,
            Err(e) => {
                failures += 1;
                eprintln!("{label}: FAIL: {e:#}");
                continue;
            }
        };
        for (i, doc) in docs.iter().enumerate() {
            total += 1;
            let where_ = if docs.len() > 1 {
                format!("{label}#{}", i + 1)
            } else {
                label.clone()
            };
            match check_document(doc, args.strict, now) {
                Ok(ev) => {
                    if !args.quiet {
                        println!("{where_}: OK {} {}", ev.uid, ev.cot_type);
                    }
                }
                Err(e) => {
                    failures += 1;
                    eprintln!("{where_}: FAIL: {e:#}");
                }
            }
        }
    }
    if !args.quiet {
        println!("{total} event(s) checked, {failures} failure(s)");
    }
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn check_document(doc: &str, strict: bool, now: Timestamp) -> anyhow::Result<CotEvent> {
    let ev = tak_cot::parse(doc)?;
    if strict {
        if ev.stale <= ev.time {
            bail!("stale ({}) is not after time ({})", ev.stale, ev.time);
        }
        to_tak_event(&ev, cli_source(), now).context("does not map onto the domain model")?;
        // and the round trip must be lossless
        let again = tak_cot::parse(&tak_cot::to_xml(&ev)?)?;
        if again != ev {
            bail!("serialisation round trip altered the event");
        }
    }
    Ok(ev)
}
