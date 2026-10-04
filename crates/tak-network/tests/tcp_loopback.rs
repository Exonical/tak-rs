//! End-to-end framing over a real TCP socket.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bytes::Bytes;
use tak_network::{
    Connector, Frame, NetworkError, StreamDecoder, TcpConnector, Transport, TransportConfig, tcp,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const EV: &str = r#"<event version="2.0" uid="loop" type="t-x-c-t" time="2024-05-01T12:00:00Z" start="2024-05-01T12:00:00Z" stale="2024-05-01T12:01:00Z"><point lat="0" lon="0" hae="0" ce="0" le="0"/><detail/></event>"#;

fn cfg() -> TransportConfig {
    TransportConfig {
        connect_timeout: Duration::from_secs(5),
        write_timeout: Duration::from_secs(5),
        read_idle_timeout: Some(Duration::from_secs(5)),
        ..TransportConfig::DEFAULT
    }
}

#[tokio::test]
async fn frames_survive_a_tcp_round_trip_in_both_directions() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        let mut t = tcp::wrap(sock, cfg());
        let mut got = Vec::new();
        while let Some(f) = t.recv().await.unwrap() {
            got.push(f.clone());
            t.send(f).await.unwrap(); // echo
            if got.len() == 3 {
                break;
            }
        }
        t.close().await.unwrap();
        got
    });

    let connector = TcpConnector::new(addr.to_string(), cfg());
    assert_eq!(connector.describe(), format!("tcp://{addr}"));
    let mut client = connector.connect().await.unwrap();
    assert!(client.id().as_str().starts_with("tcp://127.0.0.1:"));

    let frames = vec![
        Frame::xml(EV),
        Frame::TakProtobuf(Bytes::from(vec![7u8; 1000])),
        Frame::xml(format!("{EV}\n")),
    ];
    for f in &frames {
        client.send(f.clone()).await.unwrap();
    }
    let mut echoed = Vec::new();
    for _ in 0..3 {
        echoed.push(client.recv().await.unwrap().unwrap());
    }
    // The decoder strips the newline separator, so the third frame comes back
    // without it on both sides.
    let expected = vec![frames[0].clone(), frames[1].clone(), Frame::xml(EV)];
    assert_eq!(echoed, expected);
    assert_eq!(server.await.unwrap(), expected);
    // server closed → clean EOF
    assert!(matches!(client.recv().await, Ok(None)));
    client.close().await.unwrap();
}

#[tokio::test]
async fn oversized_frame_is_rejected_without_reading_it_all() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let small = TransportConfig {
        max_frame_bytes: 512,
        ..cfg()
    };
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        // declare a 10 MiB protobuf frame, then dribble bytes
        sock.write_all(&[0xBF, 0x80, 0x80, 0x80, 0x05])
            .await
            .unwrap();
        sock.write_all(&[0u8; 64]).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    let mut client = tcp::connect(addr, &small).await.unwrap();
    assert!(matches!(
        client.recv().await,
        Err(NetworkError::FrameTooLarge {
            max: 512,
            actual: 10_485_760
        })
    ));
    // sending something too big is refused locally too
    assert!(matches!(
        client
            .send(Frame::TakProtobuf(Bytes::from(vec![0; 600])))
            .await,
        Err(NetworkError::FrameTooLarge { .. })
    ));
}

#[tokio::test]
async fn idle_timeout_and_truncated_close_are_reported() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let quick = TransportConfig {
        read_idle_timeout: Some(Duration::from_millis(200)),
        ..cfg()
    };
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        sock.write_all(b"<event uid=\"half").await.unwrap();
        tokio::time::sleep(Duration::from_millis(600)).await;
        drop(sock);
    });
    let mut client = tcp::connect(addr, &quick).await.unwrap();
    assert!(matches!(client.recv().await, Err(NetworkError::Timeout(_))));

    // peer drops with a partial document buffered
    let unlimited = TransportConfig {
        read_idle_timeout: None,
        ..cfg()
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        sock.write_all(b"<event uid=\"half").await.unwrap();
    });
    let mut client = tcp::connect(addr, &unlimited).await.unwrap();
    assert!(matches!(
        client.recv().await,
        Err(NetworkError::InvalidFrame(_))
    ));
}

#[tokio::test]
async fn connection_refused_is_an_error_not_a_panic() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let err = TcpConnector::new(addr.to_string(), cfg())
        .connect()
        .await
        .err()
        .unwrap();
    assert!(matches!(err, NetworkError::Io(_)), "{err}");
}

#[tokio::test]
async fn raw_server_sees_expected_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        sock.read_to_end(&mut buf).await.unwrap();
        buf
    });
    let mut client = tcp::connect(addr, &cfg()).await.unwrap();
    client.send(Frame::xml(EV)).await.unwrap();
    client
        .send(Frame::TakProtobuf(Bytes::from_static(b"\x0a\x02\x08\x01")))
        .await
        .unwrap();
    client.close().await.unwrap();
    let bytes = server.await.unwrap();
    let mut expected = EV.as_bytes().to_vec();
    expected.push(b'\n');
    expected.extend_from_slice(&[0xBF, 4, 0x0a, 0x02, 0x08, 0x01]);
    assert_eq!(bytes, expected);
    let mut dec = StreamDecoder::new(1 << 16);
    dec.extend(&bytes);
    assert_eq!(dec.next_frame().unwrap(), Some(Frame::xml(EV)));
    assert_eq!(
        dec.next_frame().unwrap(),
        Some(Frame::TakProtobuf(Bytes::from_static(b"\x0a\x02\x08\x01")))
    );
    assert_eq!(dec.next_frame().unwrap(), None);
}
