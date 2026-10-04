//! Canonical TAK-RS domain model.
//!
//! `tak-core` defines the types every other TAK-RS crate speaks in: identifiers,
//! geodetic primitives, timestamps, the Cursor-on-Target type hierarchy, TAK
//! objects, contacts, track samples, chat messages and the [`TakEvent`] enum
//! that flows from the protocol adapters into the state engine.
//!
//! # Boundaries
//!
//! This crate deliberately knows nothing about XML, protobuf, SQLite, Dioxus,
//! MapLibre or any operating system. Wire formats are converted into these
//! types by adapter crates (`tak-cot`, `tak-proto`), persisted by `tak-storage`
//! and rendered by the applications. See `ARCHITECTURE.md` and ADR-0002.
//!
//! # Design rules
//!
//! * Values that have a domain meaning get a type: [`Latitude`], [`Callsign`],
//!   [`TakUid`], [`CotType`], [`Timestamp`] — never bare `f64` or `String`.
//! * Constructors validate. Invalid input returns [`ValidationError`]; nothing
//!   here panics on untrusted data.
//! * Unknown vendor extensions are preserved in a format-neutral
//!   [`Extensions`] tree so that TAK-RS never destroys data it does not
//!   understand.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)
)]

pub mod callsign;
pub mod chat;
pub mod color;
pub mod contact;
pub mod cot_type;
pub mod error;
pub mod event;
pub mod extension;
pub mod geo;
pub mod geometry;
pub mod how;
pub mod object;
pub mod source;
pub mod team;
pub mod time;
pub mod track;
pub mod uid;

pub use callsign::Callsign;
pub use chat::{ChatMessage, Conversation, ConversationKind};
pub use color::Argb;
pub use contact::{Contact, DeviceInfo};
pub use cot_type::{Affiliation, BattleDimension, CotRoot, CotType};
pub use error::ValidationError;
pub use event::{ControlKind, TakEvent};
pub use extension::{DetailNode, Extensions};
pub use geo::{Altitude, Distance, GeoPoint, Heading, Latitude, Longitude, Precision, Speed};
pub use geometry::Geometry;
pub use how::{How, HowCategory};
pub use object::{Link, ObjectMetadata, ObjectTimestamps, TakObject};
pub use source::{ObjectSource, Origin, TransportId, WireEncoding};
pub use team::{Role, Team};
pub use time::{Duration, Timestamp, Validity};
pub use track::{TrackPoint, TrackSource};
pub use uid::TakUid;
