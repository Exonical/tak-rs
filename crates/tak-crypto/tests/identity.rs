//! Load identities and trust stores produced the way TAK Server's
//! `makeCert.sh` does (PKCS#12 with a password, PEM for the CA).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use p12_keystore::{Certificate, KeyStore, KeyStoreEntry, PrivateKey, PrivateKeyChain};
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use tak_crypto::{ClientIdentity, TlsOptions, TrustStore, Verification, build_client_config};

struct Pki {
    ca_pem: String,
    ca_der: Vec<u8>,
    user_cert_pem: String,
    user_cert_der: Vec<u8>,
    user_key_pem: String,
    user_key_der: Vec<u8>,
}

fn pki() -> Pki {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "TAK Test Root CA");
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca_params, ca_key);

    let user_key = KeyPair::generate().unwrap();
    let mut user_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    user_params
        .distinguished_name
        .push(DnType::CommonName, "user1");
    user_params
        .distinguished_name
        .push(DnType::OrganizationName, "TAK");
    let user_cert = user_params.signed_by(&user_key, &issuer).unwrap();

    Pki {
        ca_pem: ca_cert.pem(),
        ca_der: ca_cert.der().to_vec(),
        user_cert_pem: user_cert.pem(),
        user_cert_der: user_cert.der().to_vec(),
        user_key_pem: user_key.serialize_pem(),
        user_key_der: user_key.serialize_der(),
    }
}

fn user_p12(p: &Pki, password: &str) -> Vec<u8> {
    let mut store = KeyStore::new();
    let chain = PrivateKeyChain::new(
        vec![1, 2, 3, 4],
        PrivateKey::from_der(&p.user_key_der).unwrap(),
        [
            Certificate::from_der(&p.user_cert_der).unwrap(),
            Certificate::from_der(&p.ca_der).unwrap(),
        ],
    );
    store.add_entry("user1", KeyStoreEntry::PrivateKeyChain(chain));
    store.writer(password).write().unwrap()
}

fn trust_p12(p: &Pki, password: &str) -> Vec<u8> {
    let mut store = KeyStore::new();
    store.add_entry(
        "ca",
        KeyStoreEntry::Certificate(Certificate::from_der(&p.ca_der).unwrap()),
    );
    store.writer(password).write().unwrap()
}

#[test]
fn pem_identity_loads_and_reports_leaf() {
    let p = pki();
    let id =
        ClientIdentity::from_pem(p.user_cert_pem.as_bytes(), p.user_key_pem.as_bytes()).unwrap();
    assert_eq!(id.leaf().common_name.as_deref(), Some("user1"));
    assert!(!id.leaf().is_ca);
    assert!(!id.leaf().self_signed);
    assert_eq!(id.chain().len(), 1);
    let dbg = format!("{id:?}");
    assert!(dbg.contains("redacted") && !dbg.contains("PRIVATE"));
}

#[test]
fn pkcs12_identity_loads_with_correct_password_only() {
    let p = pki();
    let bytes = user_p12(&p, "atakatak");
    let id = ClientIdentity::from_pkcs12(&bytes, "atakatak").unwrap();
    assert_eq!(id.leaf().common_name.as_deref(), Some("user1"));
    assert_eq!(id.chain().len(), 2, "leaf + CA");
    assert!(matches!(
        ClientIdentity::from_pkcs12(&bytes, "wrong"),
        Err(tak_crypto::CryptoError::Pkcs12Password)
    ));
    assert!(ClientIdentity::from_pkcs12(b"not a p12", "x").is_err());
}

#[test]
fn trust_store_accepts_pem_and_pkcs12() {
    let p = pki();
    let mut t = TrustStore::empty();
    t.add_pem(p.ca_pem.as_bytes()).unwrap();
    assert_eq!(t.len(), 1);
    assert!(t.anchors()[0].is_ca);
    let mut t2 = TrustStore::empty();
    t2.add_pkcs12(&trust_p12(&p, "atakatak"), "atakatak")
        .unwrap();
    assert_eq!(t2.len(), 1);
    assert!(TrustStore::empty().add_pem(b"").is_err());
}

#[test]
fn client_config_builds_for_every_verification_mode() {
    let p = pki();
    for mode in [Verification::Full, Verification::TrustedChainAnyName] {
        let mut trust = TrustStore::empty();
        trust.add_pem(p.ca_pem.as_bytes()).unwrap();
        let identity =
            ClientIdentity::from_pem(p.user_cert_pem.as_bytes(), p.user_key_pem.as_bytes())
                .unwrap();
        build_client_config(TlsOptions {
            trust,
            identity: Some(identity),
            verification: mode,
        })
        .unwrap();
    }
    build_client_config(TlsOptions {
        trust: TrustStore::empty(),
        identity: None,
        verification: Verification::DangerousNoVerify,
    })
    .unwrap();
    assert!(matches!(
        build_client_config(TlsOptions::default()),
        Err(tak_crypto::CryptoError::EmptyTrustStore)
    ));
}

#[test]
fn inspect_reports_fingerprint_and_validity() {
    let p = pki();
    let infos = tak_crypto::inspect_pem(p.ca_pem.as_bytes()).unwrap();
    assert_eq!(infos.len(), 1);
    let ca = &infos[0];
    assert!(ca.is_ca && ca.self_signed);
    assert_eq!(ca.sha256_fingerprint.len(), 32 * 3 - 1);
    assert!(ca.is_valid_at(time::OffsetDateTime::now_utc()));
}
