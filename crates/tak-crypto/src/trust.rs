//! Trust anchors for server verification.

use std::path::Path;

use p12_keystore::{KeyStore, KeyStoreEntry, Pkcs12ImportPolicy};
use rustls::RootCertStore;
use rustls_pki_types::CertificateDer;
use zeroize::Zeroizing;

use crate::error::CryptoError;
use crate::identity::{pkcs12_error, read};
use crate::inspect::{CertInfo, inspect_der};

/// A set of CA certificates the client trusts.
#[derive(Clone, Debug)]
pub struct TrustStore {
    roots: RootCertStore,
    anchors: Vec<CertInfo>,
}

impl Default for TrustStore {
    fn default() -> Self {
        Self {
            roots: RootCertStore::empty(),
            anchors: Vec::new(),
        }
    }
}

impl TrustStore {
    /// An empty store. Building a verifying config from it fails.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Mozilla's web PKI roots (for TAK servers behind public certificates).
    #[must_use]
    pub fn webpki_roots() -> Self {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        Self {
            roots,
            anchors: Vec::new(),
        }
    }

    /// Add one DER certificate.
    pub fn add_der(&mut self, der: &[u8]) -> Result<&mut Self, CryptoError> {
        let info = inspect_der(der)?;
        self.roots.add(CertificateDer::from(der.to_vec()))?;
        self.anchors.push(info);
        Ok(self)
    }

    /// Add every certificate in a PEM bundle.
    pub fn add_pem(&mut self, pem: &[u8]) -> Result<&mut Self, CryptoError> {
        let mut added = 0;
        for cert in rustls_pemfile::certs(&mut std::io::Cursor::new(pem)) {
            let cert = cert.map_err(|e| CryptoError::parse("CA PEM", e))?;
            self.add_der(&cert)?;
            added += 1;
        }
        if added == 0 {
            return Err(CryptoError::NoCertificate("CA PEM"));
        }
        Ok(self)
    }

    /// Add every certificate from a PKCS#12 trust store (TAK
    /// `truststore-*.p12`). Private keys, if any, are ignored.
    pub fn add_pkcs12(&mut self, der: &[u8], password: &str) -> Result<&mut Self, CryptoError> {
        let password = Zeroizing::new(password.to_owned());
        let store =
            KeyStore::from_pkcs12(der, &password, Pkcs12ImportPolicy::Raw).map_err(pkcs12_error)?;
        let mut added = 0;
        for (_, entry) in store.entries() {
            match entry {
                KeyStoreEntry::Certificate(c) => {
                    self.add_der(c.as_der())?;
                    added += 1;
                }
                KeyStoreEntry::PrivateKeyChain(chain) => {
                    for c in chain.certs() {
                        self.add_der(c.as_der())?;
                        added += 1;
                    }
                }
                KeyStoreEntry::Secret(_) => {}
            }
        }
        if added == 0 {
            return Err(CryptoError::NoCertificate("PKCS#12 trust store"));
        }
        Ok(self)
    }

    /// Add a PEM bundle from disk.
    pub fn add_pem_file(&mut self, path: &Path) -> Result<&mut Self, CryptoError> {
        let pem = read(path)?;
        self.add_pem(&pem)
    }

    /// Add a PKCS#12 trust store from disk.
    pub fn add_pkcs12_file(
        &mut self,
        path: &Path,
        password: &str,
    ) -> Result<&mut Self, CryptoError> {
        let der = read(path)?;
        self.add_pkcs12(&der, password)
    }

    /// Number of trust anchors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    /// Whether no anchors are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Summaries of explicitly added anchors (web PKI roots are not listed).
    #[must_use]
    pub fn anchors(&self) -> &[CertInfo] {
        &self.anchors
    }

    /// The underlying `rustls` root store.
    #[must_use]
    pub fn into_root_store(self) -> RootCertStore {
        self.roots
    }
}
