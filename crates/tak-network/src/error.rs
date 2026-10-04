//! Error type for the transport layer.

/// Transport and framing failures.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum NetworkError {
    /// Underlying socket error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// A frame exceeded the configured maximum size.
    #[error("frame of {actual} bytes exceeds limit of {max} bytes")]
    FrameTooLarge {
        /// Configured maximum.
        max: usize,
        /// Observed (or declared) size.
        actual: usize,
    },
    /// Bytes that are neither CoT XML nor a TAK Protocol frame.
    #[error("invalid frame: {0}")]
    InvalidFrame(String),
    /// The peer closed the connection (or we did).
    #[error("connection closed")]
    Closed,
    /// An operation exceeded its deadline.
    #[error("timed out after {0:?}")]
    Timeout(std::time::Duration),
    /// Endpoint string could not be parsed.
    #[error("invalid endpoint `{endpoint}`: {reason}")]
    InvalidEndpoint {
        /// The offending string.
        endpoint: String,
        /// Why.
        reason: String,
    },
    /// Requested a protocol this crate does not implement.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

impl From<tokio::time::error::Elapsed> for NetworkError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        Self::Timeout(std::time::Duration::ZERO)
    }
}
