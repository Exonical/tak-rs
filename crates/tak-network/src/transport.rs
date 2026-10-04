//! The [`Transport`] abstraction.

use std::time::Duration;

use async_trait::async_trait;
use tak_core::TransportId;

use crate::error::NetworkError;
use crate::frame::Frame;

/// Tunables shared by all transports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportConfig {
    /// Largest frame accepted or sent. TAK Server defaults to roughly 1 MiB;
    /// data-package manifests and large KML can approach it.
    pub max_frame_bytes: usize,
    /// Deadline for establishing a connection.
    pub connect_timeout: Duration,
    /// Deadline for a single `send` to be flushed to the socket.
    pub write_timeout: Duration,
    /// If no bytes arrive for this long, `recv` fails with
    /// [`NetworkError::Timeout`] so supervisors can reconnect. `None`
    /// disables the check.
    pub read_idle_timeout: Option<Duration>,
}

impl TransportConfig {
    /// Reasonable defaults for a TAK Server streaming connection.
    pub const DEFAULT: Self = Self {
        max_frame_bytes: 2 * 1024 * 1024,
        connect_timeout: Duration::from_secs(10),
        write_timeout: Duration::from_secs(10),
        read_idle_timeout: Some(Duration::from_secs(90)),
    };
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A bidirectional, message-oriented connection to a TAK peer.
///
/// Implementations must be cancellation-safe: dropping a pending `recv` or
/// `send` future must not corrupt framing.
#[async_trait]
pub trait Transport: Send {
    /// Identifier for this connection (used in [`tak_core::ObjectSource`]).
    fn id(&self) -> &TransportId;

    /// Write one frame and flush it.
    async fn send(&mut self, frame: Frame) -> Result<(), NetworkError>;

    /// Wait for the next frame. `Ok(None)` means the peer closed cleanly.
    async fn recv(&mut self) -> Result<Option<Frame>, NetworkError>;

    /// Shut the connection down gracefully.
    async fn close(&mut self) -> Result<(), NetworkError>;
}

/// Something that can establish a [`Transport`], possibly repeatedly.
///
/// Reconnect supervisors hold a `Connector` and call it after each failure.
#[async_trait]
pub trait Connector: Send + Sync {
    /// Human-readable target, e.g. `tcp://host:8087`.
    fn describe(&self) -> String;

    /// Establish a new connection.
    async fn connect(&self) -> Result<Box<dyn Transport>, NetworkError>;
}
