//! Build a `rustls::ClientConfig` from a trust store, an optional identity
//! and an explicit verification policy.

use std::sync::Arc;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};

use crate::error::CryptoError;
use crate::identity::ClientIdentity;
use crate::trust::TrustStore;

/// How strictly to verify the server certificate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verification {
    /// Chain must lead to a trust anchor and the name must match. Default.
    #[default]
    Full,
    /// Chain must lead to a trust anchor; the hostname/IP is **not** checked.
    /// Common with TAK servers whose certificate says `CN=takserver` but are
    /// reached by IP address.
    TrustedChainAnyName,
    /// No verification at all. Only for debugging against a server whose
    /// trust store you do not have; logs a warning on every build.
    DangerousNoVerify,
}

/// Everything needed to build a TLS client configuration.
#[derive(Debug, Default)]
pub struct TlsOptions {
    /// Trust anchors. May be empty only with [`Verification::DangerousNoVerify`].
    pub trust: TrustStore,
    /// Client certificate for mutual TLS (TAK servers on 8089 require it).
    pub identity: Option<ClientIdentity>,
    /// Verification policy.
    pub verification: Verification,
}

/// Build the `rustls` configuration.
pub fn build_client_config(opts: TlsOptions) -> Result<Arc<ClientConfig>, CryptoError> {
    let provider = crate::crypto_provider();
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?;

    let builder = match opts.verification {
        Verification::Full => {
            if opts.trust.is_empty() {
                return Err(CryptoError::EmptyTrustStore);
            }
            builder.with_root_certificates(opts.trust.into_root_store())
        }
        Verification::TrustedChainAnyName => {
            if opts.trust.is_empty() {
                return Err(CryptoError::EmptyTrustStore);
            }
            tracing::warn!("TLS: server hostname verification disabled (chain is still verified)");
            let inner = WebPkiServerVerifier::builder_with_provider(
                Arc::new(opts.trust.into_root_store()),
                provider.clone(),
            )
            .build()
            .map_err(|e| CryptoError::parse("trust store", e))?;
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AnyNameVerifier { inner }))
        }
        Verification::DangerousNoVerify => {
            tracing::warn!(
                "TLS: SERVER CERTIFICATE VERIFICATION DISABLED — connection is not authenticated"
            );
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerifier { provider }))
        }
    };

    let config = match opts.identity {
        Some(identity) => {
            tracing::info!(subject = %identity.leaf().subject, "TLS: using client certificate");
            let (chain, key) = identity.into_parts();
            builder.with_client_auth_cert(chain, key)?
        }
        None => builder.with_no_client_auth(),
    };
    Ok(Arc::new(config))
}

/// Parse a host string (DNS name or IP literal) into a `rustls` server name.
pub fn server_name(host: &str) -> Result<ServerName<'static>, CryptoError> {
    ServerName::try_from(host.to_owned())
        .map_err(|_| CryptoError::InvalidServerName(host.to_owned()))
}

/// Verifies the chain with `WebPKI` but ignores name mismatches.
#[derive(Debug)]
struct AnyNameVerifier {
    inner: Arc<WebPkiServerVerifier>,
}

impl ServerCertVerifier for AnyNameVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        match self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            Err(Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            )) => {
                tracing::debug!(?server_name, "TLS: ignoring certificate name mismatch");
                Ok(ServerCertVerified::assertion())
            }
            other => other,
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// Accepts any certificate. Handshake signatures are still checked so the
/// peer at least proves possession of the presented key.
#[derive(Debug)]
struct NoVerifier {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}
