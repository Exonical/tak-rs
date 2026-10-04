//! Fuzz the CoT parser, serialiser and domain adapter.
//!
//! Invariants checked on every input that parses:
//! * `to_xml` succeeds and re-parses to an identical `CotEvent`;
//! * `to_tak_event` never panics, and when it succeeds the domain event can be
//!   re-encoded.

#![no_main]

use libfuzzer_sys::fuzz_target;
use tak_core::{ObjectSource, Timestamp};
use tak_cot::adapter::{from_tak_event, to_tak_event};
use tak_cot::{parse, to_xml};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(event) = parse(text) else {
        return;
    };
    let xml = to_xml(&event).expect("a parsed event must serialise");
    let again = parse(&xml).expect("serialised event must re-parse");
    assert_eq!(again, event, "round trip changed the event");

    let now = Timestamp::UNIX_EPOCH;
    if let Ok(domain) = to_tak_event(&event, ObjectSource::LOCAL, now) {
        let encoded = from_tak_event(&domain, now).expect("domain event must encode");
        to_xml(&encoded).expect("encoded domain event must serialise");
    }
});
