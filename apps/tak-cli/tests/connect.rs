//! Black-box tests of `tak connect|contacts|status|cert` against an in-process
//! TCP "server" that speaks CoT XML.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn tak() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tak"))
}

fn fixture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/tak-cot/tests/fixtures/valid")
        .join(name);
    std::fs::read_to_string(p).unwrap()
}

/// Spawn a server that sends the given XML documents to every client, then
/// echoes anything it receives back and holds the connection open.
async fn server(docs: Vec<String>) -> (u16, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        for d in &docs {
            sock.write_all(d.as_bytes()).await.unwrap();
        }
        sock.flush().await.unwrap();
        let mut received = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match sock.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    received.extend_from_slice(&buf[..n]);
                    if sock.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            }
        }
        received
    });
    (port, task)
}

fn run_blocking(cmd: &mut Command) -> std::process::Output {
    cmd.output().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn contacts_lists_callsigns_seen_over_tcp() {
    let (port, srv) = server(vec![fixture("atak_sa.xml"), fixture("wintak_sa.xml")]).await;
    let out = tokio::task::spawn_blocking(move || {
        run_blocking(tak().args([
            "contacts",
            "--server",
            &format!("127.0.0.1:{port}:tcp"),
            "--for",
            "2",
        ]))
    })
    .await
    .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("ALPHA"), "{stdout}");
    assert!(stdout.contains("CALLSIGN"), "{stdout}");
    assert_eq!(stdout.lines().count(), 3, "{stdout}");
    srv.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn status_reports_xml_mode_and_counts() {
    let (port, srv) = server(vec![fixture("atak_sa.xml"), fixture("marker_hostile.xml")]).await;
    let out = tokio::task::spawn_blocking(move || {
        run_blocking(tak().args([
            "status",
            "--server",
            &format!("127.0.0.1:{port}"),
            "--for",
            "2",
            "--json",
        ]))
    })
    .await
    .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["connected"], true);
    assert_eq!(v["wire_mode"], "cot-xml");
    assert_eq!(v["events_received"], 2);
    assert_eq!(v["contacts"], 1);
    assert_eq!(v["objects"], 1);
    srv.await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_streams_events_and_sends_file() {
    let (port, srv) = server(vec![fixture("atak_sa.xml")]).await;
    let send = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/tak-cot/tests/fixtures/valid/marker_hostile.xml");
    let out = tokio::task::spawn_blocking(move || {
        run_blocking(tak().args([
            "connect",
            "--server",
            &format!("127.0.0.1:{port}:tcp"),
            "--duration",
            "2",
            "--format",
            "jsonl",
            "--send",
            send.to_str().unwrap(),
        ]))
    })
    .await
    .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<serde_json::Value> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // The SA from the server plus the echo of what we sent.
    assert_eq!(lines.len(), 2, "{stdout}");
    assert_eq!(lines[0]["type"], "a-f-G-U-C");
    assert_eq!(lines[0]["domain"]["event"], "contact_updated");
    let received = srv.await.unwrap();
    let received = String::from_utf8(received).unwrap();
    assert!(received.contains("<event"), "server got: {received}");
    assert!(received.contains("a-h-G"), "server got: {received}");
}

#[test]
fn connect_fails_fast_when_nothing_listens() {
    // Reserve a port and close it so nothing listens there.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let out = run_blocking(tak().args([
        "status",
        "--server",
        &format!("127.0.0.1:{port}:tcp"),
        "--for",
        "1",
    ]));
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("connected            false"), "{stdout}");
}

#[test]
fn ssl_without_trust_anchor_is_refused() {
    let out = run_blocking(tak().args(["status", "--server", "127.0.0.1:8089:ssl", "--for", "1"]));
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("trust anchor"), "{err}");
}

#[test]
fn cert_inspect_pem_and_p12() {
    use rcgen::{CertificateParams, DnType, KeyPair, SanType};
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params
        .distinguished_name
        .push(DnType::CommonName, "inspect-me");
    params.subject_alt_names = vec![SanType::DnsName("tak.example".try_into().unwrap())];
    let cert = params.self_signed(&key).unwrap();
    let dir = std::env::temp_dir().join(format!("tak-cli-cert-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pem = dir.join("cert.pem");
    std::fs::write(&pem, format!("{}{}", cert.pem(), key.serialize_pem())).unwrap();

    let out = run_blocking(tak().args(["cert", "inspect", pem.to_str().unwrap()]));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("CN=inspect-me"), "{stdout}");
    assert!(stdout.contains("tak.example"), "{stdout}");
    assert!(stdout.contains("self-signed"), "{stdout}");
    assert!(stdout.contains("private key: present"), "{stdout}");
    assert!(
        !stdout.contains("PRIVATE KEY"),
        "key material leaked: {stdout}"
    );

    let out = run_blocking(tak().args(["cert", "inspect", "--json", pem.to_str().unwrap()]));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["certificates"][0]["common_name"], "inspect-me");
    assert_eq!(v["has_private_key"], true);
    std::fs::remove_dir_all(&dir).ok();
}
