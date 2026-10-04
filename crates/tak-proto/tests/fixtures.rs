//! Every valid CoT fixture must survive XML → proto → bytes → proto → XML with
//! identical event attributes, point and detail content (order-insensitive).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tak-cot/tests/fixtures/valid")
}

/// Protobuf carries neither attribute order nor numeric spelling
/// (`course="0"` vs `course="0.0"`), so compare attribute-sorted,
/// number-normalised renderings of each top-level detail node.
fn sorted_detail(event: &tak_cot::CotEvent) -> Vec<String> {
    fn sort_attrs(n: &mut tak_core::DetailNode) {
        for (_, v) in &mut n.attributes {
            if let Ok(f) = v.parse::<f64>() {
                *v = format!("{f:?}");
            }
        }
        n.attributes.sort();
        n.children.iter_mut().for_each(sort_attrs);
    }
    let mut v: Vec<String> = event
        .detail
        .iter()
        .map(|n| {
            let mut n = n.clone();
            sort_attrs(&mut n);
            tak_cot::node_to_xml(&n).unwrap()
        })
        .collect();
    v.sort();
    v
}

#[test]
fn all_valid_fixtures_round_trip_through_tak_protocol() {
    let mut count = 0;
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let path = entry.unwrap().path();
        let xml = std::fs::read_to_string(&path).unwrap();
        let event = tak_cot::parse(&xml).unwrap();
        let msg = tak_proto::to_proto(&event)
            .unwrap_or_else(|e| panic!("{}: to_proto: {e}", path.display()));
        let bytes = tak_proto::encode(&msg);
        let decoded = tak_proto::decode(&bytes).unwrap();
        assert_eq!(decoded, msg, "{}", path.display());
        let back = tak_proto::from_proto(&decoded)
            .unwrap_or_else(|e| panic!("{}: from_proto: {e}", path.display()));

        assert_eq!(back.uid, event.uid, "{}", path.display());
        assert_eq!(back.cot_type, event.cot_type, "{}", path.display());
        assert_eq!(back.how, event.how, "{}", path.display());
        assert_eq!(back.time, event.time, "{}", path.display());
        assert_eq!(back.start, event.start, "{}", path.display());
        assert_eq!(back.stale, event.stale, "{}", path.display());
        assert_eq!(back.access, event.access, "{}", path.display());
        assert_eq!(back.point.lat, event.point.lat, "{}", path.display());
        assert_eq!(back.point.lon, event.point.lon, "{}", path.display());
        assert_eq!(
            sorted_detail(&back),
            sorted_detail(&event),
            "{}",
            path.display()
        );

        // and the result is still valid CoT XML that parses back identically
        let xml2 = tak_cot::to_xml(&back).unwrap();
        let again = tak_cot::parse(&xml2).unwrap();
        assert_eq!(again, back, "{}", path.display());
        count += 1;
    }
    assert!(count >= 17, "expected the fixture set, saw {count}");
}

#[test]
fn hoisted_fields_are_absent_when_detail_is_empty() {
    let xml = std::fs::read_to_string(fixtures().join("ping.xml")).unwrap();
    let event = tak_cot::parse(&xml).unwrap();
    let msg = tak_proto::to_proto(&event).unwrap();
    let d = msg.cot_event.unwrap().detail.unwrap();
    assert!(d.contact.is_none() && d.group.is_none() && d.track.is_none());
}
