//! Transport layer for TAK-RS.
//!
//! This crate knows how TAK bytes are *framed* and *moved*, not what they
//! mean. It provides:
//!
//! * [`Frame`] — one wire message, either a CoT XML document or a TAK
//!   Protocol (protobuf) payload, as raw bytes;
//! * [`StreamDecoder`] / [`StreamEncoder`] — split a byte stream into frames
//!   and vice versa, supporting the plain XML stream used before protocol
//!   negotiation and the TAK Protocol v1 stream framing (`0xBF` + varint
//!   length) used afterwards, with automatic detection;
//! * [`Transport`] — the async send/receive abstraction every connection type
//!   implements, plus [`Connector`] for things that can (re)establish one;
//! * [`StreamTransport`] — a [`Transport`] over any `AsyncRead + AsyncWrite`
//!   (TCP today, TLS in `tak-transport`);
//! * [`TcpConnector`] — plain TCP, as used by TAK Server's 8087 streaming
//!   port and the ATAK `stcp` mesh endpoint;
//! * [`Endpoint`] — parser for TAK `host:port:proto` endpoint strings.
//!
//! Decoding frames into domain events is done by `tak-cot` / `tak-proto`;
//! connection supervision (reconnect, protocol negotiation) lives in
//! `tak-transport`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod endpoint;
pub mod error;
pub mod frame;
pub mod stream;
pub mod tcp;
pub mod transport;

pub use endpoint::{Endpoint, Protocol};
pub use error::NetworkError;
pub use frame::{Frame, FrameKind, StreamDecoder, StreamEncoder};
pub use stream::StreamTransport;
pub use tcp::TcpConnector;
pub use transport::{Connector, Transport, TransportConfig};
