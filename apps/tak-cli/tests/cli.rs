//! Black-box tests of the `tak` binary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn tak() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tak"))
}

fn run_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = tak()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/tak-cot/tests/fixtures")
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

#[test]
fn encode_then_decode_round_trips() {
    let enc = tak()
        .args([
            "cot",
            "encode",
            "--uid",
            "CLI-1",
            "--type",
            "a-f-G-U-C",
            "--lat",
            "38.9",
            "--lon",
            "-77.0",
            "--hae",
            "15",
            "--callsign",
            "ALPHA",
            "--team",
            "Cyan",
            "--role",
            "Team Lead",
            "--remarks",
            "hi & bye",
            "--detail",
            "<vendor x=\"1\"><y/></vendor>",
        ])
        .output()
        .unwrap();
    assert!(enc.status.success(), "{}", stderr(&enc));
    let xml = stdout(&enc);
    assert!(xml.starts_with("<?xml"), "{xml}");
    assert!(xml.contains("<contact callsign=\"ALPHA\"/>"));
    assert!(xml.contains("<__group name=\"Cyan\" role=\"Team Lead\"/>"));
    assert!(xml.contains("<remarks>hi &amp; bye</remarks>"));
    assert!(xml.contains("<vendor x=\"1\"><y/></vendor>"));

    let dec = run_with_stdin(&["cot", "decode"], &xml);
    assert!(dec.status.success(), "{}", stderr(&dec));
    let s = stdout(&dec);
    assert!(s.contains("uid:      CLI-1"), "{s}");
    assert!(s.contains("(Contact)"), "{s}");
    assert!(s.contains("callsign: ALPHA"), "{s}");
    assert!(s.contains("hae=15m"), "{s}");
    assert!(s.contains("vendor"), "{s}");

    let json = run_with_stdin(&["cot", "decode", "--format", "json", "--domain"], &xml);
    assert!(json.status.success(), "{}", stderr(&json));
    let v: serde_json::Value = serde_json::from_str(&stdout(&json)).unwrap();
    assert_eq!(v["uid"], "CLI-1");
    assert_eq!(v["point"]["hae"], 15.0);
    assert!(v["point"]["ce"].is_null());
    assert_eq!(v["domain"]["event"], "contact_updated");
    assert_eq!(v["domain"]["callsign"], "ALPHA");
    assert_eq!(v["detail"][3]["name"], "vendor");

    let val = run_with_stdin(&["cot", "validate", "--strict"], &xml);
    assert!(val.status.success(), "{}", stderr(&val));
    assert!(stdout(&val).contains("1 event(s) checked, 0 failure(s)"));
}

#[test]
fn validate_accepts_every_valid_fixture_and_rejects_every_malformed_one() {
    let valid: Vec<_> = std::fs::read_dir(fixtures().join("valid"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let out = tak()
        .args(["cot", "validate", "--strict", "--quiet"])
        .args(&valid)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "");

    for bad in std::fs::read_dir(fixtures().join("malformed")).unwrap() {
        let bad = bad.unwrap().path();
        let out = tak().args(["cot", "validate"]).arg(&bad).output().unwrap();
        if bad.ends_with("two_events.xml") {
            // a stream of two documents is fine for the CLI, which splits them
            assert!(out.status.success(), "{}", stderr(&out));
            assert!(stdout(&out).contains("2 event(s) checked, 0 failure(s)"));
            continue;
        }
        assert_eq!(out.status.code(), Some(1), "{}", bad.display());
        assert!(stderr(&out).contains("FAIL"), "{}", bad.display());
    }
}

#[test]
fn decode_splits_a_captured_stream() {
    let a = std::fs::read_to_string(fixtures().join("valid/atak_sa.xml")).unwrap();
    let b = std::fs::read_to_string(fixtures().join("valid/ping.xml")).unwrap();
    let capture = format!("{a}\n{b}\n{a}");
    let out = run_with_stdin(&["cot", "decode", "--format", "json"], &capture);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out).lines().count(), 3);

    let out = run_with_stdin(&["cot", "decode", "--format", "xml"], &capture);
    assert!(out.status.success());
    assert_eq!(stdout(&out).matches("</event>").count(), 3);

    let out = run_with_stdin(&["cot", "decode"], "<event uid=\"x\">");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("do not form a complete <event>"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn encode_rejects_bad_coordinates_and_strict_validate_catches_stale_before_time() {
    let out = tak()
        .args(["cot", "encode", "--lat", "95", "--lon", "0"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("lat"), "{}", stderr(&out));

    let xml = r#"<event version="2.0" uid="x" type="a-f-G" how="m-g" time="2024-05-01T12:00:00Z" start="2024-05-01T12:00:00Z" stale="2024-05-01T11:00:00Z"><point lat="1" lon="2" hae="0" ce="0" le="0"/><detail/></event>"#;
    assert!(run_with_stdin(&["cot", "validate"], xml).status.success());
    let strict = run_with_stdin(&["cot", "validate", "--strict"], xml);
    assert_eq!(strict.status.code(), Some(1));
    assert!(stderr(&strict).contains("stale"), "{}", stderr(&strict));
}
