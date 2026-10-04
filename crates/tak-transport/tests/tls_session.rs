//! End-to-end against an in-process TAK-like server: mutual TLS, TAK Protocol
//! negotiation, protobuf exchange, and supervisor reconnects.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, SanType};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tak_core::{TakUid, Timestamp, TransportId};
use tak_crypto::{ClientIdentity, TlsOptions, TrustStore, Verification};
use tak_network::{Frame, StreamTransport, TcpConnector, Transport as _, TransportConfig};
use tak_transport::negotiate::{TYPE_REQUEST, TYPE_RESPONSE, TYPE_SUPPORT, control_event};
use tak_transport::{Session, Supervisor, SupervisorEvent, TlsConnector, WireMode};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

struct Pki {
    ca_pem: String,
    server_der: CertificateDer<'static>,
    server_key: Vec<u8>,
    client_pem: String,
    client_key_pem: String,
}

fn pki() -> Pki {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca.distinguished_name.push(DnType::CommonName, "Test CA");
    let ca_cert = ca.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca, ca_key);

    let server_key = KeyPair::generate().unwrap();
    let mut server = CertificateParams::new(Vec::<String>::new()).unwrap();
    server
        .distinguished_name
        .push(DnType::CommonName, "takserver");
    server.subject_alt_names = vec![
        SanType::DnsName("localhost".try_into().unwrap()),
        SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)),
    ];
    let server_cert = server.signed_by(&server_key, &issuer).unwrap();

    let client_key = KeyPair::generate().unwrap();
    let mut client = CertificateParams::new(Vec::<String>::new()).unwrap();
    client.distinguished_name.push(DnType::CommonName, "user1");
    let client_cert = client.signed_by(&client_key, &issuer).unwrap();

    Pki {
        ca_pem: ca_cert.pem(),
        server_der: server_cert.der().clone(),
        server_key: server_key.serialize_der(),
        client_pem: client_cert.pem(),
        client_key_pem: client_key.serialize_pem(),
    }
}

fn server_config(p: &Pki) -> Arc<ServerConfig> {
    let provider = tak_crypto::crypto_provider();
    let mut roots = RootCertStore::empty();
    for c in rustls_pemfile::certs(&mut p.ca_pem.as_bytes()) {
        roots.add(c.unwrap()).unwrap();
    }
    let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .unwrap();
    let cfg = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_client_cert_verifier(verifier)
        .with_single_cert(
            vec![p.server_der.clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(p.server_key.clone())),
        )
        .unwrap();
    Arc::new(cfg)
}

fn client_config(p: &Pki, verification: Verification) -> Arc<rustls::ClientConfig> {
    let mut trust = TrustStore::empty();
    trust.add_pem(p.ca_pem.as_bytes()).unwrap();
    let identity =
        ClientIdentity::from_pem(p.client_pem.as_bytes(), p.client_key_pem.as_bytes()).unwrap();
    tak_crypto::build_client_config(TlsOptions {
        trust,
        identity: Some(identity),
        verification,
    })
    .unwrap()
}

fn sa_event() -> tak_cot::CotEvent {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tak-cot/tests/fixtures/valid/atak_sa.xml"
    );
    tak_cot::parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn fast() -> TransportConfig {
    TransportConfig {
        connect_timeout: Duration::from_secs(5),
        write_timeout: Duration::from_secs(5),
        read_idle_timeout: Some(Duration::from_secs(5)),
        ..TransportConfig::DEFAULT
    }
}

fn server_control(cot_type: &str, element: &str, attrs: &[(&str, &str)]) -> Frame {
    let ev = control_event(
        &TakUid::new("TAK-SERVER").unwrap(),
        cot_type,
        Timestamp::now(),
        element,
        attrs,
    )
    .unwrap();
    Frame::xml(tak_cot::to_xml(&ev).unwrap())
}

#[tokio::test]
async fn mtls_negotiates_protobuf_and_exchanges_events() {
    let p = pki();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let acceptor = TlsAcceptor::from(server_config(&p));

    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let peer_cn = tls
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|c| c.first())
            .map(|c| tak_crypto::inspect_der(c).unwrap().common_name.unwrap());
        assert_eq!(
            peer_cn.as_deref(),
            Some("user1"),
            "client cert must be presented"
        );
        let mut t = StreamTransport::new(tls, TransportId::new("server"), fast());

        t.send(server_control(
            TYPE_SUPPORT,
            "TakProtocolSupport",
            &[("version", "1")],
        ))
        .await
        .unwrap();
        let req = t.recv().await.unwrap().unwrap();
        let req = tak_cot::parse(req.as_xml_str().unwrap()).unwrap();
        assert_eq!(req.cot_type.as_str(), TYPE_REQUEST);
        assert_eq!(req.uid.as_str(), "CLIENT-UID");
        t.send(server_control(
            TYPE_RESPONSE,
            "TakResponse",
            &[("status", "true")],
        ))
        .await
        .unwrap();

        // Server now speaks protobuf.
        let sa = tak_proto::to_proto(&sa_event()).unwrap();
        t.send(Frame::TakProtobuf(tak_proto::encode(&sa)))
            .await
            .unwrap();

        // Client must answer in protobuf; echo it back.
        let from_client = t.recv().await.unwrap().unwrap();
        assert!(
            matches!(from_client, Frame::TakProtobuf(_)),
            "client must switch to protobuf"
        );
        t.send(from_client).await.unwrap();
        // One junk frame the client must survive.
        t.send(Frame::TakProtobuf(bytes::Bytes::from_static(&[
            0xFF, 0xFF, 0xFF,
        ])))
        .await
        .unwrap();
        t.send(Frame::xml("<event uid=\"after-junk\" type=\"a-f-G\" version=\"2.0\" time=\"2024-05-01T12:00:00Z\" start=\"2024-05-01T12:00:00Z\" stale=\"2024-05-01T12:05:00Z\"><point lat=\"1\" lon=\"2\" hae=\"0\" ce=\"1\" le=\"1\"/></event>"))
            .await
            .unwrap();
        t.close().await.unwrap();
    });

    let connector = TlsConnector::new(
        "localhost",
        port,
        client_config(&p, Verification::Full),
        fast(),
    )
    .unwrap();
    let transport = tak_network::Connector::connect(&connector).await.unwrap();
    let mut session = Session::new(transport, TakUid::new("CLIENT-UID").unwrap());
    assert_eq!(session.wire_mode(), WireMode::CotXml);

    let sa = session.recv().await.unwrap().expect("SA event");
    assert_eq!(sa.uid.as_str(), sa_event().uid.as_str());
    assert_eq!(session.wire_mode(), WireMode::TakProtocolV1);

    let mut mine = sa_event();
    mine.uid = TakUid::new("CLIENT-UID").unwrap();
    session.send(&mine).await.unwrap();
    let echo = session.recv().await.unwrap().expect("echo");
    assert_eq!(echo.uid.as_str(), "CLIENT-UID");

    let after = session.recv().await.unwrap().expect("event after junk");
    assert_eq!(after.uid.as_str(), "after-junk");
    assert!(session.recv().await.unwrap().is_none(), "clean close");
    let stats = session.stats();
    assert_eq!(stats.decode_errors, 1);
    assert_eq!(stats.events_in, 3);
    assert_eq!(stats.events_out, 1);
    server.await.unwrap();
}

#[tokio::test]
async fn full_verification_rejects_name_mismatch_but_chain_only_mode_accepts() {
    let p = pki();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let acceptor = TlsAcceptor::from(server_config(&p));
    tokio::spawn(async move {
        loop {
            let (tcp, _) = listener.accept().await.unwrap();
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                if let Ok(mut tls) = acceptor.accept(tcp).await {
                    let _ = tokio::io::AsyncWriteExt::shutdown(&mut tls).await;
                }
            });
        }
    });
    // Certificate is for localhost/127.0.0.1; connect via a name that is not in the SAN.
    // Use the IP-literal "127.0.0.2"? Not routable on all CI hosts; instead build a connector
    // whose SNI/verification name differs from what the certificate says.
    let strict = TlsConnector::new(
        "127.0.0.1",
        port,
        client_config(&p, Verification::Full),
        fast(),
    )
    .unwrap();
    assert!(strict.connect_tls().await.is_ok(), "IP SAN should verify");

    let wrong_name = client_config(&p, Verification::Full);
    let connector = tokio_rustls::TlsConnector::from(wrong_name);
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let name = rustls_pki_types::ServerName::try_from("not-the-server.example").unwrap();
    assert!(
        connector.connect(name, tcp).await.is_err(),
        "wrong name must fail under Full"
    );

    let relaxed = client_config(&p, Verification::TrustedChainAnyName);
    let connector = tokio_rustls::TlsConnector::from(relaxed);
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let name = rustls_pki_types::ServerName::try_from("not-the-server.example").unwrap();
    assert!(
        connector.connect(name, tcp).await.is_ok(),
        "chain-only mode ignores the name"
    );

    let untrusted = {
        let other = pki();
        client_config(&other, Verification::TrustedChainAnyName)
    };
    let connector = tokio_rustls::TlsConnector::from(untrusted);
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let name = rustls_pki_types::ServerName::try_from("localhost").unwrap();
    assert!(
        connector.connect(name, tcp).await.is_err(),
        "chain-only mode still verifies the chain"
    );
}

#[tokio::test]
async fn supervisor_reconnects_after_drop_and_stops_on_cancel() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        // First connection: drop immediately.
        let (first, _) = listener.accept().await.unwrap();
        drop(first);
        // Second connection: deliver one XML event and hold.
        let (second, _) = listener.accept().await.unwrap();
        let mut t = tak_network::tcp::wrap(second, fast());
        t.send(Frame::xml(tak_cot::to_xml(&sa_event()).unwrap()))
            .await
            .unwrap();
        let _ = t.recv().await; // wait until the client goes away
    });

    let connector = Arc::new(TcpConnector::new(addr.to_string(), fast()));
    let mut sup = Supervisor::new(connector, TakUid::new("CLIENT").unwrap());
    sup.backoff.initial = Duration::from_millis(20);
    sup.backoff.max = Duration::from_millis(50);
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut handle = sup.spawn(cancel.clone());

    let mut saw = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let ev = tokio::time::timeout_at(deadline, handle.events.recv())
            .await
            .unwrap()
            .unwrap();
        let label = match &ev {
            SupervisorEvent::Connected { .. } => "connected",
            SupervisorEvent::Disconnected { .. } => "disconnected",
            SupervisorEvent::Received { event, .. } => {
                assert_eq!(event.uid.as_str(), sa_event().uid.as_str());
                "received"
            }
            SupervisorEvent::WireMode(_) => "mode",
            SupervisorEvent::Stopped => "stopped",
            _ => "other",
        };
        saw.push(label);
        if label == "received" {
            break;
        }
    }
    assert_eq!(saw, ["connected", "disconnected", "connected", "received"]);

    cancel.cancel();
    let mut stopped = false;
    while let Some(ev) = tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
        .await
        .unwrap()
    {
        if matches!(ev, SupervisorEvent::Stopped) {
            stopped = true;
            break;
        }
    }
    assert!(stopped);
    handle.shutdown().await;
}
