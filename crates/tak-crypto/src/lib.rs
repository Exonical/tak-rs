//! TLS identity, trust and configuration for TAK connections.
//!
//! TAK deployments hand users a client certificate (usually a PKCS#12
//! `user.p12`, password-protected) and a trust store (`truststore-root.p12`
//! holding only certificates, or a PEM bundle). This crate loads both without
//! OpenSSL, builds a [`rustls::ClientConfig`] for mutual TLS, and exposes
//! [`CertInfo`] for `tak cert inspect`.
//!
//! Security notes
//! * Private key material lives in [`rustls_pki_types::PrivateKeyDer`] (which
//!   zeroizes on drop) and is never `Debug`-printed; PKCS#12 passwords are
//!   held in [`zeroize::Zeroizing`] buffers.
//! * Full certificate verification is the default. The relaxed modes in
//!   [`Verification`] must be chosen explicitly and are logged at `warn`.
//! * `rustls` with the `ring` provider only; no system TLS library.

pub mod config;
pub mod error;
pub mod identity;
pub mod inspect;
pub mod trust;

pub use config::{TlsOptions, Verification, build_client_config};
pub use error::CryptoError;
pub use identity::ClientIdentity;
pub use inspect::{CertInfo, inspect_der, inspect_pem};
pub use trust::TrustStore;

/// The crypto provider every `rustls` config in TAK-RS uses.
#[must_use]
pub fn crypto_provider() -> std::sync::Arc<rustls::crypto::CryptoProvider> {
    std::sync::Arc::new(rustls::crypto::ring::default_provider())
}
