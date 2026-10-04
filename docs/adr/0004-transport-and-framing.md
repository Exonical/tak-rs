# ADR 0004 — `Transport` trait and frame-level encoding detection

**Status:** Accepted (2026-10)

## Context
TAK connections are TCP (8087), TLS (8089), UDP multicast mesh, and QUIC.
A streaming connection begins as concatenated CoT XML and may switch to TAK
Protocol v1 framing (`0xBF`, varint length, protobuf) after negotiation.
State and applications must not care which.

## Decision
`tak-network` defines object-safe `Transport { id, send, recv, close }` and
`Connector { connect }` traits (via `async_trait`) exchanging `Frame`
(`CotXml(Bytes)` | `TakProtobuf(Bytes)`). `StreamDecoder` detects the
encoding per frame from the first byte, so negotiation is a higher-level
concern (`tak-transport`). `StreamTransport<S>` is generic over any
`AsyncRead + AsyncWrite` so TLS reuses it unchanged. `TransportConfig` bounds
frame size and applies connect/write/idle timeouts. Datagram framing
(`BF 01 BF`) is provided for the UDP mesh.

## Consequences
* Decoding frames to events is the caller's job; `tak-network` has no
  dependency on `tak-cot` or `tak-proto`.
* Dynamic dispatch per frame is negligible relative to socket I/O.
* A reconnect supervisor can be written once against `Connector`.
