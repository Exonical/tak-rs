//! TAK Protocol (version 1) message codec.
//!
//! TAK Protocol wraps a CoT event in a compact protobuf `TakMessage` instead
//! of XML. This crate provides:
//!
//! * [`message`]: hand-written `prost` mirrors of the official `.proto`
//!   schema (vendored in `proto/`), so no `protoc` is required to build.
//! * [`encode`] / [`decode`]: bytes ⇄ [`TakMessage`].
//! * [`adapter`]: [`tak_cot::CotEvent`] ⇄ [`TakMessage`]. The `<detail>`
//!   children the schema hoists into typed fields are hoisted when they are
//!   fully representable; everything else travels in `xmlDetail` and is parsed
//!   with `tak-cot`, so unknown detail stays lossless.
//!
//! Framing (`0xBF` magic + varint length) lives in `tak-network`; protocol
//! negotiation lives in the transport layer.

pub mod adapter;
pub mod error;
pub mod message;

use bytes::Bytes;
use prost::Message as _;

pub use adapter::{from_proto, lossy_fields, to_proto};
pub use error::ProtoError;
pub use message::{
    Contact, CotEvent, Detail, Group, PrecisionLocation, Status, TakControl, TakMessage, Takv,
    Track,
};

/// The only TAK Protocol version this crate speaks.
pub const TAK_PROTO_VERSION: u32 = 1;

/// Serialise a [`TakMessage`] to protobuf bytes (without stream framing).
#[must_use]
pub fn encode(message: &TakMessage) -> Bytes {
    Bytes::from(message.encode_to_vec())
}

/// Parse protobuf bytes (without stream framing) into a [`TakMessage`].
///
/// Callers must bound `bytes` themselves (the transport frame limit does
/// this); `prost` bounds recursion depth.
pub fn decode(bytes: &[u8]) -> Result<TakMessage, ProtoError> {
    Ok(TakMessage::decode(bytes)?)
}

impl TakMessage {
    /// A `TakControl`-only message advertising the given version range.
    #[must_use]
    pub fn control(min_proto_version: u32, max_proto_version: u32, contact_uid: &str) -> Self {
        Self {
            tak_control: Some(TakControl {
                min_proto_version,
                max_proto_version,
                contact_uid: contact_uid.to_owned(),
            }),
            cot_event: None,
        }
    }

    /// Wrap a protobuf CoT event.
    #[must_use]
    pub fn event(event: CotEvent) -> Self {
        Self {
            tak_control: None,
            cot_event: Some(event),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn control_message_matches_hand_encoded_bytes() {
        // TakMessage.takControl (field 1, len 4) { minProtoVersion=1, maxProtoVersion=1 }
        let bytes = encode(&TakMessage::control(1, 1, ""));
        assert_eq!(bytes.as_ref(), &[0x0A, 0x04, 0x08, 0x01, 0x10, 0x01]);
        let back = decode(&bytes).unwrap();
        assert_eq!(back.tak_control.unwrap().max_proto_version, 1);
        assert!(back.cot_event.is_none());
    }

    #[test]
    fn garbage_is_rejected_not_panicked() {
        assert!(decode(&[0xFF, 0xFF, 0xFF, 0xFF]).is_err());
        assert!(decode(&[0x0A, 0x10, 0x01]).is_err());
        assert!(decode(&[]).is_ok());
    }
}
