//! Human-readable certificate summaries (no key material).

use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;
use sha2::{Digest as _, Sha256};
use time::OffsetDateTime;
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::{FromDer as _, X509Certificate};

use crate::error::CryptoError;

/// Summary of an X.509 certificate for display and logging.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CertInfo {
    /// Full subject distinguished name.
    pub subject: String,
    /// Subject common name, if present.
    pub common_name: Option<String>,
    /// Full issuer distinguished name.
    pub issuer: String,
    /// Serial number as upper-case hex with `:` separators.
    pub serial: String,
    /// Start of validity.
    pub not_before: OffsetDateTime,
    /// End of validity.
    pub not_after: OffsetDateTime,
    /// Subject alternative names (DNS names, IP addresses, emails, URIs).
    pub subject_alt_names: Vec<String>,
    /// Whether basic constraints mark this as a CA certificate.
    pub is_ca: bool,
    /// Whether the certificate is self-signed (subject == issuer).
    pub self_signed: bool,
    /// Public key algorithm OID in dotted form.
    pub public_key_algorithm: String,
    /// SHA-256 fingerprint of the DER encoding, upper-case hex with `:`.
    pub sha256_fingerprint: String,
}

impl CertInfo {
    /// Whether `now` falls inside the validity window.
    #[must_use]
    pub fn is_valid_at(&self, now: OffsetDateTime) -> bool {
        now >= self.not_before && now <= self.not_after
    }
}

/// Inspect a DER-encoded certificate.
pub fn inspect_der(der: &[u8]) -> Result<CertInfo, CryptoError> {
    let (_, cert) =
        X509Certificate::from_der(der).map_err(|e| CryptoError::parse("certificate DER", e))?;
    let common_name = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|a| a.as_str().ok())
        .map(str::to_owned);
    let subject_alt_names = cert
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|ext| {
            ext.value
                .general_names
                .iter()
                .map(general_name_to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(CertInfo {
        subject: cert.subject().to_string(),
        common_name,
        issuer: cert.issuer().to_string(),
        serial: colon_hex(cert.raw_serial()),
        not_before: cert.validity().not_before.to_datetime(),
        not_after: cert.validity().not_after.to_datetime(),
        subject_alt_names,
        is_ca: cert.is_ca(),
        self_signed: cert.subject() == cert.issuer(),
        public_key_algorithm: cert.public_key().algorithm.algorithm.to_id_string(),
        sha256_fingerprint: colon_hex(&Sha256::digest(der)),
    })
}

/// Inspect every certificate in a PEM bundle, in order.
pub fn inspect_pem(pem: &[u8]) -> Result<Vec<CertInfo>, CryptoError> {
    let mut out = Vec::new();
    for cert in CertificateDer::pem_slice_iter(pem) {
        let cert = cert.map_err(|e| CryptoError::parse("certificate PEM", e))?;
        out.push(inspect_der(&cert)?);
    }
    if out.is_empty() {
        return Err(CryptoError::NoCertificate("PEM input"));
    }
    Ok(out)
}

fn general_name_to_string(name: &GeneralName<'_>) -> String {
    match name {
        GeneralName::DNSName(s) => format!("DNS:{s}"),
        GeneralName::RFC822Name(s) => format!("email:{s}"),
        GeneralName::URI(s) => format!("URI:{s}"),
        GeneralName::IPAddress(b) => match b.len() {
            4 => format!("IP:{}.{}.{}.{}", b[0], b[1], b[2], b[3]),
            16 => {
                let mut segs = [0u16; 8];
                for (i, seg) in segs.iter_mut().enumerate() {
                    *seg = u16::from_be_bytes([b[2 * i], b[2 * i + 1]]);
                }
                format!("IP:{}", std::net::Ipv6Addr::from(segs))
            }
            _ => format!("IP:{}", colon_hex(b)),
        },
        other => format!("{other:?}"),
    }
}

pub(crate) fn colon_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}
