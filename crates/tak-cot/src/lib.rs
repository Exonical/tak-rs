//! Cursor-on-Target (CoT) XML codec for TAK-RS.
//!
//! This crate owns everything XML-specific about CoT:
//!
//! * [`parse()`] turns untrusted CoT XML into a [`CotEvent`], enforcing size,
//!   depth and node-count limits and rejecting DTDs and unknown entities.
//! * [`to_xml`] serialises a [`CotEvent`] back to XML.
//! * [`detail`] provides typed views over well-known `<detail>` children
//!   (`<contact>`, `<__group>`, `<takv>`, `<track>`, `<__chat>`, ...).
//! * [`adapter`] converts between [`CotEvent`] and the wire-neutral domain
//!   model in [`tak_core`] (`Contact`, `TakObject`, `ChatMessage`, `TakEvent`).
//!
//! The wire model keeps every `<detail>` child it does not understand as a
//! [`tak_core::DetailNode`] tree so that unknown ATAK/WinTAK/iTAK extensions
//! survive a decode → encode round trip unchanged.
//!
//! Nothing in this crate performs I/O; framing CoT documents on a stream is
//! the job of `tak-network`.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::panic
    )
)]

pub mod adapter;
pub mod detail;
pub mod error;
pub mod event;
pub mod parse;
pub mod write;

pub use error::CotError;
pub use event::{COT_VERSION, CotEvent, CotPoint, UNKNOWN_SENTINEL};
pub use parse::{
    ParseLimits, parse, parse_detail_fragment, parse_detail_fragment_with_limits, parse_with_limits,
};
pub use write::{WriteOptions, node_to_xml, nodes_to_xml, to_xml, to_xml_with};
