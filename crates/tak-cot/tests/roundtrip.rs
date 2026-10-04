//! Property tests: any well-formed `CotEvent` survives write → parse.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use proptest::prelude::*;
use tak_core::{CotType, DetailNode, Extensions, How, TakUid, Timestamp};
use tak_cot::{CotEvent, CotPoint, WriteOptions, parse, to_xml, to_xml_with};

fn xml_name() -> impl Strategy<Value = String> {
    "[A-Za-z_][A-Za-z0-9_.:-]{0,12}"
}

/// Attribute values: XML normalises `\t`, `\n`, `\r` in attributes to spaces,
/// so those are excluded (they cannot round trip by specification).
fn attr_value() -> impl Strategy<Value = String> {
    "[ -~\u{a0}-\u{2ff}\u{4e00}-\u{4eff}]{0,24}"
}

/// Text content: must contain at least one non-whitespace character (the
/// parser treats whitespace-only text as absent) and no `\r` (EOL
/// normalisation).
fn text_value() -> impl Strategy<Value = String> {
    "[ -~\n\t]{0,10}[!-~\u{4e00}-\u{4eff}][ -~\n\t]{0,10}"
}

fn leaf() -> impl Strategy<Value = DetailNode> {
    (
        xml_name(),
        prop::collection::vec((xml_name(), attr_value()), 0..4),
        prop::option::of(text_value()),
    )
        .prop_map(|(name, attrs, text)| {
            let mut n = DetailNode::new(name);
            for (k, v) in attrs {
                n.set_attr(k, v);
            }
            n.text = text;
            n
        })
}

fn node() -> impl Strategy<Value = DetailNode> {
    leaf().prop_recursive(4, 24, 4, |inner| {
        (
            xml_name(),
            prop::collection::vec((xml_name(), attr_value()), 0..4),
            prop::collection::vec(inner, 1..4),
        )
            .prop_map(|(name, attrs, children)| {
                let mut n = DetailNode::new(name);
                for (k, v) in attrs {
                    n.set_attr(k, v);
                }
                n.children = children;
                n
            })
    })
}

fn uid() -> impl Strategy<Value = TakUid> {
    "[A-Za-z0-9][A-Za-z0-9 ._:@-]{0,40}[A-Za-z0-9]".prop_map(|s| TakUid::new(s).unwrap())
}

fn cot_type() -> impl Strategy<Value = CotType> {
    "[a-z](-[A-Za-z0-9]{1,3}){1,6}".prop_map(|s| CotType::new(s).unwrap())
}

fn how() -> impl Strategy<Value = Option<How>> {
    prop::option::of(prop::sample::select(vec![
        How::machine_gps(),
        How::human_entered(),
        How::human_gigo(),
        How::new("m-f").unwrap(),
    ]))
}

fn timestamp() -> impl Strategy<Value = Timestamp> {
    (0i64..4_000_000_000_000).prop_map(|ms| Timestamp::from_unix_millis(ms).unwrap())
}

fn point() -> impl Strategy<Value = CotPoint> {
    (
        -90.0f64..=90.0,
        -180.0f64..=180.0,
        prop_oneof![Just(9_999_999.0), -500.0f64..20_000.0],
        prop_oneof![Just(9_999_999.0), 0.0f64..10_000.0],
        prop_oneof![Just(9_999_999.0), 0.0f64..10_000.0],
    )
        .prop_map(|(lat, lon, hae, ce, le)| CotPoint {
            lat,
            lon,
            hae,
            ce,
            le,
        })
}

prop_compose! {
    fn event()(
        uid in uid(),
        cot_type in cot_type(),
        how in how(),
        time in timestamp(),
        start in timestamp(),
        stale in timestamp(),
        point in point(),
        access in prop::option::of(attr_value()),
        extra in prop::collection::vec(
            ("[a-wyz][A-Za-z0-9_-]{0,8}".prop_filter("reserved", |k| {
                !matches!(k.as_str(), "version" | "uid" | "type" | "how" | "time" | "start" | "stale"
                    | "access" | "opex" | "qos" | "caveat" | "releaseableTo")
            }), attr_value()),
            0..3,
        ),
        detail in prop::collection::vec(node(), 0..5),
    ) -> CotEvent {
        let mut ev = CotEvent::new(uid, cot_type, point, time, stale);
        ev.how = how;
        ev.start = start;
        ev.access = access;
        let mut seen = std::collections::HashSet::new();
        ev.extra_attributes = extra.into_iter().filter(|(k, _)| seen.insert(k.clone())).collect();
        ev.detail = Extensions::from_nodes(detail);
        ev
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn write_then_parse_is_identity(ev in event()) {
        let xml = to_xml(&ev).unwrap();
        let back = parse(&xml).unwrap();
        prop_assert_eq!(&back, &ev);
        let pretty = to_xml_with(&ev, &WriteOptions::PRETTY).unwrap();
        let back = parse(&pretty).unwrap();
        prop_assert_eq!(&back, &ev);
    }

    #[test]
    fn parser_never_panics_on_arbitrary_input(s in "\\PC{0,512}") {
        let _ = parse(&s);
    }

    #[test]
    fn parser_never_panics_on_mutated_valid_input(ev in event(), idx in 0usize..4096, byte in any::<u8>()) {
        let mut bytes = to_xml(&ev).unwrap().into_bytes();
        let i = idx % bytes.len();
        bytes[i] = byte;
        if let Ok(s) = std::str::from_utf8(&bytes) {
            let _ = parse(s);
        }
    }
}
