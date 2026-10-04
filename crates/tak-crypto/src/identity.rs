//! Client (mTLS) identity: a certificate chain plus its private key.

use std::fmt;
use std::path::Path;

use p12_keystore::{KeyStore, Pkcs12ImportPolicy};
use rustls_pki_types::pem::{self, PemObject};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use zeroize::Zeroizing;

use crate::error::CryptoError;
use crate::inspect::{CertInfo, inspect_der};

/// A client certificate chain and private key, ready for `rustls`.
///
/// `Debug` prints the leaf subject and fingerprint only.
pub struct ClientIdentity {
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
    leaf: CertInfo,
}

impl ClientIdentity {
    /// Load from PEM: `cert_pem` holds the leaf (and optionally intermediates,
    /// leaf first); `key_pem` holds a PKCS#8, RSA (PKCS#1) or SEC1 key.
    pub fn from_pem(cert_pem: &[u8], key_pem: &[u8]) -> Result<Self, CryptoError> {
        let mut chain = Vec::new();
        for cert in CertificateDer::pem_slice_iter(cert_pem) {
            chain.push(cert.map_err(|e| CryptoError::parse("certificate PEM", e))?);
        }
        if chain.is_empty() {
            return Err(CryptoError::NoCertificate("certificate PEM"));
        }
        let key = PrivateKeyDer::from_pem_slice(key_pem).map_err(|e| match e {
            pem::Error::NoItemsFound => CryptoError::NoPrivateKey("private key PEM"),
            other => CryptoError::parse("private key PEM", other),
        })?;
        Self::new(chain, key)
    }

    /// Load from a PKCS#12 archive (TAK `user.p12`). The archive must contain
    /// exactly one private key with its certificate chain.
    pub fn from_pkcs12(der: &[u8], password: &str) -> Result<Self, CryptoError> {
        let password = Zeroizing::new(password.to_owned());
        let store = KeyStore::from_pkcs12(der, &password, Pkcs12ImportPolicy::Strict)
            .map_err(pkcs12_error)?;
        let (_, keychain) = store
            .private_key_chain()
            .ok_or(CryptoError::NoPrivateKey("PKCS#12"))?;
        let key_der = Zeroizing::new(keychain.key().as_der().to_vec());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der.to_vec()));
        let mut chain: Vec<CertificateDer<'static>> = keychain
            .certs()
            .iter()
            .map(|c| CertificateDer::from(c.as_der().to_vec()))
            .collect();
        if chain.is_empty() {
            return Err(CryptoError::NoCertificate("PKCS#12"));
        }
        // p12-keystore does not guarantee order; put the non-CA end-entity first.
        if let Some(pos) = chain
            .iter()
            .position(|c| inspect_der(c).is_ok_and(|i| !i.is_ca))
        {
            chain.swap(0, pos);
        }
        Self::new(chain, key)
    }

    /// Read PEM files from disk.
    pub fn from_pem_files(cert: &Path, key: &Path) -> Result<Self, CryptoError> {
        let cert_pem = read(cert)?;
        let key_pem = Zeroizing::new(read(key)?);
        Self::from_pem(&cert_pem, &key_pem)
    }

    /// Read a PKCS#12 file from disk.
    pub fn from_pkcs12_file(path: &Path, password: &str) -> Result<Self, CryptoError> {
        let der = Zeroizing::new(read(path)?);
        Self::from_pkcs12(&der, password)
    }

    /// Build from already-parsed parts. The first certificate must be the leaf.
    pub fn new(
        chain: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
    ) -> Result<Self, CryptoError> {
        let leaf_der = chain.first().ok_or(CryptoError::NoCertificate("chain"))?;
        let leaf = inspect_der(leaf_der)?;
        Ok(Self { chain, key, leaf })
    }

    /// Summary of the end-entity certificate.
    #[must_use]
    pub fn leaf(&self) -> &CertInfo {
        &self.leaf
    }

    /// The certificate chain, leaf first.
    #[must_use]
    pub fn chain(&self) -> &[CertificateDer<'static>] {
        &self.chain
    }

    /// Consume into the parts `rustls` wants.
    #[must_use]
    pub fn into_parts(self) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
        (self.chain, self.key)
    }

    /// Clone the parts `rustls` wants (the key is copied into a fresh
    /// zeroizing buffer).
    #[must_use]
    pub fn to_parts(&self) -> (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) {
        (self.chain.clone(), self.key.clone_key())
    }
}

impl fmt::Debug for ClientIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientIdentity")
            .field("subject", &self.leaf.subject)
            .field("sha256", &self.leaf.sha256_fingerprint)
            .field("chain_len", &self.chain.len())
            .field("key", &"<redacted>")
            .finish()
    }
}

pub(crate) fn pkcs12_error(e: p12_keystore::error::Error) -> CryptoError {
    match e {
        p12_keystore::error::Error::MacError(_) | p12_keystore::error::Error::UnpadError => {
            CryptoError::Pkcs12Password
        }
        other => CryptoError::parse("PKCS#12", other),
    }
}

pub(crate) fn read(path: &Path) -> Result<Vec<u8>, CryptoError> {
    std::fs::read(path).map_err(|source| CryptoError::Io {
        path: path.display().to_string(),
        source,
    })
}
