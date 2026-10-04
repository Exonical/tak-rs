//! Fixture-driven interoperability tests.
//!
//! Every file under `tests/fixtures/valid` must parse, survive a
//! parse → write → parse round trip unchanged, and adapt to a `TakEvent`.
//! Every file under `tests/fixtures/malformed` must be rejected.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::semicolon_if_nothing_returned,
    clippy::redundant_closure_for_method_calls
)]

use std::fs;
use std::path::{Path, PathBuf};

use tak_core::{
    ConversationKind, Geometry, ObjectSource, TakEvent, Timestamp, TransportId, WireEncoding,
};
use tak_cot::adapter::{Classification, classify, from_tak_event, to_tak_event};
use tak_cot::{CotError, WriteOptions, parse, to_xml, to_xml_with};

fn fixture_dir(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(kind)
}

fn fixtures(kind: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_dir(fixture_dir(kind))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read_to_string(&p).unwrap())
        })
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no fixtures found in {kind}");
    out
}

fn source() -> ObjectSource {
    ObjectSource::remote(TransportId::new("fixture"), WireEncoding::CotXml)
}

#[test]
fn every_valid_fixture_parses_and_round_trips() {
    for (name, xml) in fixtures("valid") {
        let event = parse(&xml).unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
        for opts in [WriteOptions::WIRE, WriteOptions::PRETTY] {
            let out =
                to_xml_with(&event, &opts).unwrap_or_else(|e| panic!("{name}: write failed: {e}"));
            let again =
                parse(&out).unwrap_or_else(|e| panic!("{name}: re-parse failed: {e}\n{out}"));
            assert_eq!(
                again, event,
                "{name}: round trip changed the event ({opts:?})"
            );
        }
    }
}

#[test]
fn every_valid_fixture_adapts_to_a_domain_event() {
    let now = Timestamp::now();
    for (name, xml) in fixtures("valid") {
        let event = parse(&xml).unwrap();
        let domain = to_tak_event(&event, source(), now)
            .unwrap_or_else(|e| panic!("{name}: adapter failed: {e}"));
        if !matches!(domain, TakEvent::ChatReceived(_)) {
            assert_eq!(domain.source(), &source(), "{name}");
        }
        // encode back and re-adapt: the domain view must be stable
        let encoded =
            from_tak_event(&domain, now).unwrap_or_else(|e| panic!("{name}: encode failed: {e}"));
        let xml2 = to_xml(&encoded).unwrap();
        let domain2 = to_tak_event(&parse(&xml2).unwrap(), source(), now)
            .unwrap_or_else(|e| panic!("{name}: re-adapt failed: {e}\n{xml2}"));
        match (&domain, &domain2) {
            (TakEvent::ContactUpdated(a), TakEvent::ContactUpdated(b)) => {
                assert_eq!(a, b, "{name}")
            }
            (TakEvent::ObjectUpdated(a), TakEvent::ObjectUpdated(b)) => assert_eq!(a, b, "{name}"),
            (TakEvent::ChatReceived(a), TakEvent::ChatReceived(b)) => assert_eq!(a, b, "{name}"),
            (TakEvent::ObjectRemoved { uid: a, .. }, TakEvent::ObjectRemoved { uid: b, .. }) => {
                assert_eq!(a, b, "{name}");
            }
            (TakEvent::Control { cot_type: a, .. }, TakEvent::Control { cot_type: b, .. }) => {
                assert_eq!(a, b, "{name}");
            }
            (a, b) => panic!("{name}: variant changed after round trip: {a:?} vs {b:?}"),
        }
    }
}

#[test]
fn every_malformed_fixture_is_rejected() {
    for (name, xml) in fixtures("malformed") {
        let err = match parse(&xml) {
            Err(e) => e,
            Ok(ev) => panic!("{name}: expected rejection, parsed {ev:?}"),
        };
        if name.starts_with("doctype") || name.starts_with("external_entity") {
            assert!(matches!(err, CotError::DoctypeNotAllowed), "{name}: {err}");
        }
        if name == "undefined_entity.xml" {
            assert!(matches!(err, CotError::UnknownEntity(_)), "{name}: {err}");
        }
        if name == "two_events.xml" {
            assert!(matches!(err, CotError::TrailingContent), "{name}: {err}");
        }
    }
}

fn load(name: &str) -> tak_cot::CotEvent {
    let xml = fs::read_to_string(fixture_dir("valid").join(name)).unwrap();
    parse(&xml).unwrap()
}

#[test]
fn classification_matches_expectations() {
    let table = [
        ("atak_sa.xml", Classification::Contact),
        ("wintak_sa.xml", Classification::Contact),
        ("itak_sa.xml", Classification::Contact),
        ("unknown_extensions.xml", Classification::Contact),
        ("geochat_all_chat_rooms.xml", Classification::Chat),
        ("geochat_direct.xml", Classification::Chat),
        ("marker_hostile.xml", Classification::Object),
        ("shape_circle.xml", Classification::Object),
        ("shape_polygon.xml", Classification::Object),
        ("shape_rectangle.xml", Classification::Object),
        ("route.xml", Classification::Object),
        ("emergency_911.xml", Classification::Object),
        ("sensor_video.xml", Classification::Object),
        ("delete.xml", Classification::Delete),
        ("ping.xml", Classification::Control),
        (
            "takserver_protocol_negotiation.xml",
            Classification::Control,
        ),
        ("takserver_protocol_request.xml", Classification::Control),
    ];
    for (name, expected) in table {
        assert_eq!(classify(&load(name)), expected, "{name}");
    }
    // The table must cover every fixture so new ones get a decision.
    let names: Vec<_> = fixtures("valid").into_iter().map(|(n, _)| n).collect();
    for n in &names {
        assert!(
            table.iter().any(|(t, _)| t == n),
            "{n} missing from classification table"
        );
    }
}

#[test]
fn wintak_quirks_are_tolerated() {
    // integer sentinels, two-digit fractional seconds, self-closing with spaces
    let ev = load("wintak_sa.xml");
    assert_eq!(ev.point.hae_metres(), None);
    assert_eq!(ev.point.ce_metres(), None);
    let TakEvent::ContactUpdated(c) = to_tak_event(&ev, source(), Timestamp::now()).unwrap() else {
        panic!("expected contact");
    };
    assert_eq!(c.callsign.as_str(), "BRAVO-OPS");
    assert_eq!(c.team, Some(tak_core::Team::DarkBlue));
    assert_eq!(c.role, Some(tak_core::Role::Hq));
    assert_eq!(c.speed.map(|s| s.as_mps()), Some(0.0));
    assert_eq!(
        c.device.as_ref().unwrap().platform.as_deref(),
        Some("WinTAK-CIV")
    );
    // extra attribute on <contact> survives re-encode
    let back = from_tak_event(&TakEvent::ContactUpdated(c), Timestamp::now()).unwrap();
    assert!(back.detail.contains("precisionlocation"));
}

#[test]
fn unknown_extensions_survive_verbatim() {
    let ev = load("unknown_extensions.xml");
    assert_eq!(
        ev.extra_attributes,
        vec![("vendorAttr".to_owned(), "kept".to_owned())]
    );
    let tele = ev.detail.get("acme:telemetry").unwrap();
    assert_eq!(tele.attr("xmlns:acme"), Some("urn:acme"));
    assert_eq!(tele.text.as_deref(), Some("raw <payload> & stuff"));
    assert_eq!(ev.detail.get("deep").unwrap().depth(), 9);
    assert_eq!(
        ev.detail.get("text-only").unwrap().text.as_deref(),
        Some("   spaced   out   ")
    );
    assert_eq!(
        ev.detail.get("entities").unwrap().text.as_deref(),
        Some("<tag> & \"quotes\" 'apos' ☺ ☺")
    );
    assert_eq!(
        ev.detail.get("unicode").unwrap().attr("name"),
        Some("Ünïcödé 日本語 🙂")
    );
    assert!(ev.detail.get("empty").unwrap().text.is_none());

    let TakEvent::ContactUpdated(c) = to_tak_event(&ev, source(), Timestamp::now()).unwrap() else {
        panic!("expected contact");
    };
    assert_eq!(c.callsign.as_str(), "EXT & CO");
    assert_eq!(c.team, Some(tak_core::Team::Magenta));
    for name in [
        "__group_future",
        "acme:telemetry",
        "deep",
        "empty",
        "text-only",
        "entities",
        "unicode",
    ] {
        assert!(
            c.extensions.contains(name),
            "{name} should be preserved as an extension"
        );
    }
    let xml =
        to_xml(&from_tak_event(&TakEvent::ContactUpdated(c), Timestamp::now()).unwrap()).unwrap();
    let again = parse(&xml).unwrap();
    assert_eq!(
        again.detail.get("acme:telemetry"),
        ev.detail.get("acme:telemetry")
    );
    assert_eq!(again.detail.get("deep"), ev.detail.get("deep"));
}

#[test]
fn shapes_and_routes_decode_geometry() {
    let now = Timestamp::now();
    let TakEvent::ObjectUpdated(circle) =
        to_tak_event(&load("shape_circle.xml"), source(), now).unwrap()
    else {
        panic!()
    };
    assert!(matches!(circle.geometry, Geometry::Circle { radius_m, .. } if radius_m == 500.0));
    assert_eq!(circle.metadata.stroke_weight, Some(4.0));
    assert!(circle.metadata.extensions.contains("__shapeExtras"));

    let TakEvent::ObjectUpdated(poly) =
        to_tak_event(&load("shape_polygon.xml"), source(), now).unwrap()
    else {
        panic!()
    };
    assert!(matches!(&poly.geometry, Geometry::Polygon(v) if v.len() == 4));

    let TakEvent::ObjectUpdated(rect) =
        to_tak_event(&load("shape_rectangle.xml"), source(), now).unwrap()
    else {
        panic!()
    };
    assert!(matches!(rect.geometry, Geometry::Rectangle { .. }));

    let TakEvent::ObjectUpdated(route) = to_tak_event(&load("route.xml"), source(), now).unwrap()
    else {
        panic!()
    };
    assert!(matches!(&route.geometry, Geometry::Polyline(v) if v.len() == 3));
    assert!(route.metadata.extensions.contains("link_attr"));
}

#[test]
fn chat_fixtures_decode() {
    let now = Timestamp::now();
    let TakEvent::ChatReceived(all) =
        to_tak_event(&load("geochat_all_chat_rooms.xml"), source(), now).unwrap()
    else {
        panic!()
    };
    assert_eq!(all.conversation.kind, ConversationKind::Broadcast);
    assert_eq!(all.text, "Radio check, all stations & relays. Over.");
    let TakEvent::ChatReceived(dm) =
        to_tak_event(&load("geochat_direct.xml"), source(), now).unwrap()
    else {
        panic!()
    };
    assert_eq!(dm.conversation.kind, ConversationKind::Direct);
    assert_eq!(dm.conversation.name, "BRAVO-OPS");
    assert_eq!(dm.recipients.len(), 1);
}

#[test]
fn delete_targets_linked_uid() {
    let TakEvent::ObjectRemoved { uid, .. } =
        to_tak_event(&load("delete.xml"), source(), Timestamp::now()).unwrap()
    else {
        panic!()
    };
    assert_eq!(uid.as_str(), "7f2c9b1e-8d4a-4e6b-b1c3-5a6d7e8f9a0b");
}
