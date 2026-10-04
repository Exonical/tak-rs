//! Error type for sessions and supervisors.

/// Failures above the raw transport.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransportError {
    /// Socket, framing or timeout failure.
    #[error(transparent)]
    Network(#[from] tak_network::NetworkError),
    /// TLS configuration failure.
    #[error(transparent)]
    Crypto(#[from] tak_crypto::CryptoError),
    /// Could not encode an outbound event.
    #[error("encode failed: {0}")]
    Encode(String),
    /// Peer violated the negotiation protocol.
    #[error("protocol negotiation failed: {0}")]
    Negotiation(String),
    /// The session was closed (by the peer or by cancellation).
    #[error("session closed")]
    Closed,
}
