//! Error type for identity and trust loading.

/// Errors from `tak-crypto`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CryptoError {
    /// PEM/DER input could not be parsed.
    #[error("{what}: {reason}")]
    Parse {
        /// What was being parsed (`"certificate PEM"`, `"PKCS#12"`, …).
        what: &'static str,
        /// Underlying reason, never containing key material.
        reason: String,
    },
    /// PKCS#12 MAC check failed — almost always a wrong password.
    #[error("PKCS#12 integrity check failed (wrong password?)")]
    Pkcs12Password,
    /// The input had no private key.
    #[error("no private key found in {0}")]
    NoPrivateKey(&'static str),
    /// The input had no certificate.
    #[error("no certificate found in {0}")]
    NoCertificate(&'static str),
    /// The trust store is empty and verification was requested.
    #[error("trust store is empty; add a CA bundle or choose a different verification mode")]
    EmptyTrustStore,
    /// `rustls` rejected the configuration (bad key/cert pairing, etc.).
    #[error("TLS configuration rejected: {0}")]
    Tls(#[from] rustls::Error),
    /// A server name is not a valid DNS name or IP address.
    #[error("invalid server name {0:?}")]
    InvalidServerName(String),
    /// Filesystem error while reading a file.
    #[error("{path}: {source}")]
    Io {
        /// The path that failed.
        path: String,
        /// The I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl CryptoError {
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn parse(what: &'static str, reason: impl ToString) -> Self {
        Self::Parse {
            what,
            reason: reason.to_string(),
        }
    }
}
