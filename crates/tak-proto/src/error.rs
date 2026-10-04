//! Error type for TAK Protocol encoding, decoding and adaptation.

use tak_core::ValidationError;
use tak_cot::CotError;

/// Errors from `tak-proto`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProtoError {
    /// The bytes are not a valid protobuf `TakMessage`.
    #[error("protobuf decode failed: {0}")]
    Decode(#[from] prost::DecodeError),
    /// The message carries neither `takControl` nor `cotEvent`.
    #[error("TakMessage contains neither takControl nor cotEvent")]
    Empty,
    /// The `xmlDetail` fragment or a hoisted detail is malformed.
    #[error(transparent)]
    Cot(#[from] CotError),
    /// A field failed domain validation (uid, type, how, time).
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// A millisecond timestamp does not fit the supported range.
    #[error("{field} timestamp {millis} ms is out of range")]
    TimeOutOfRange {
        /// Which event attribute.
        field: &'static str,
        /// The offending value.
        millis: u64,
    },
    /// The event uses a field that cannot be expressed in TAK Protocol.
    #[error("CoT event field `{0}` has no TAK Protocol representation")]
    NotRepresentable(&'static str),
}
