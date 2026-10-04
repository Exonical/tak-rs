# Architecture

TAK-RS is a layered Rust SDK. Each layer may depend only on the layers below
it; wire formats, transports and UI toolkits are leaves that never leak
upward.

```
                 ┌────────────────────────────────────────────┐
  applications   │ tak-cli        tak-agent        tak-client │  thin shells
                 └──────┬───────────────┬───────────────┬─────┘
                        │               │               │
                 ┌──────▼───────────────▼───────────────▼─────┐
  services       │ tak-state   tak-transport   tak-storage*   │  *planned
                 │ tak-crypto  tak-missions*   tak-plugins*   │
                 └──────┬──────────────┬─────────────────┬────┘
                        │              │                 │
                 ┌──────▼──────┐ ┌─────▼──────┐  ┌───────▼─────┐
  codecs/wire    │  tak-cot    │ │ tak-proto  │  │ tak-network │
                 │  CoT XML    │ │ protobuf   │  │ framing+TCP │
                 └──────┬──────┘ └─────┬──────┘  └───────┬─────┘
                        │              │                 │
                 ┌──────▼──────────────▼─────────────────▼─────┐
  domain         │                 tak-core                    │
                 │  TakUid · GeoPoint · CotType · Contact ·    │
                 │  TakObject · ChatMessage · TakEvent ·       │
                 │  Extensions (format-neutral detail tree)    │
                 └─────────────────────────────────────────────┘
```

## Layer rules

1. **`tak-core` is wire-neutral.** It contains no XML, protobuf, Dioxus,
   Android, iOS, MapLibre or SQLite types and depends only on `time`, `uuid`,
   `thiserror` and optionally `serde`. Every value type validates on
   construction (`Latitude::new(91.0)` is an error, never a panic).
2. **Codecs convert to canonical types.** `tak-cot` (and later `tak-proto`)
   parse bytes into a wire-shaped struct (`CotEvent`) and then *adapt* it to
   `tak-core` (`Contact`, `TakObject`, `ChatMessage`, `TakEvent`). Anything the
   adapter does not understand is carried in `Extensions` so it can be
   re-emitted byte-for-byte in meaning.
3. **Transports move frames, not meaning.** `tak-network` knows how a TAK
   stream is framed (`</event>`-terminated XML, or `0xBF` + varint + protobuf)
   and exposes `Transport { send, recv, close }`. It never parses XML.
4. **State is transport-independent.** `tak-state` (phase 2) consumes
   `TakEvent`s from any source — TCP, TLS, UDP mesh, a replay file — and keeps
   contacts/objects/chat with stale-time handling. Offline is the default
   state, not an error state.
5. **UI is a presentation layer.** `tak-client` renders state and dispatches
   intents; it owns no business logic. The map (MapLibre) sits behind a trait so
   a headless build has no map dependency.
6. **Headless must always work.** Everything an operator needs must be
   reachable from `tak-cli`/`tak-agent` without a display.

## Data flow (receive path)

```
socket ──► StreamDecoder ──► Frame::CotXml(bytes)
                               │
                               ▼
                      tak_cot::parse(&str) ──► CotEvent        (wire shape, lossless)
                               │
                               ▼
              tak_cot::adapter::to_tak_event(..) ──► TakEvent  (ContactUpdated | ObjectUpdated
                               │                                | ChatReceived | ObjectRemoved | Control)
                               ▼
                         tak-state / application
```

The send path is the mirror image: `from_tak_event` → `to_xml` →
`StreamEncoder` → socket. Unknown detail nodes picked up on receive are
re-emitted on send, so TAK-RS can relay traffic it does not understand.

## Crate responsibilities

### `tak-core`
* Strong value types: `TakUid`, `Callsign`, `Latitude`/`Longitude`/`GeoPoint`,
  `Altitude`, `Precision`, `Speed`, `Heading`, `Timestamp`, `Validity`,
  `CotType` (+ `Affiliation`, `BattleDimension`), `How`, `Team`, `Role`, `Argb`.
* Entities: `Contact`, `TakObject` (+ `Geometry`), `TrackPoint`, `ChatMessage`,
  `Conversation`.
* `TakEvent`: the single event enum every producer emits and every consumer
  handles. `#[non_exhaustive]` so new kinds can be added.
* `Extensions` / `DetailNode`: an ordered, format-neutral element tree used to
  preserve unknown detail.
* `ObjectSource`: where a thing came from (local, remote transport id, import)
  and in which `WireEncoding`.

### `tak-cot`
* `parse` / `parse_with_limits`: quick-xml based, DTD-free, entity-safe, with
  byte/depth/node/attribute limits (`ParseLimits`).
* `to_xml` / `to_xml_with`: deterministic serialiser, wire or pretty.
* `detail`: typed views (`ContactDetail`, `GroupDetail`, `TakvDetail`,
  `TrackDetail`, `ChatDetail`, `LinkDetail`, …) via `KnownDetail`/`DetailExt`.
  Each typed view keeps unknown attributes/children in `extra`/`other_children`.
* `adapter`: `classify`, `to_tak_event` / `from_tak_event` and the per-entity
  conversions; `delete_event`, `ping_event`, `control_event` helpers.

### `tak-network`
* `Frame` (`CotXml` | `TakProtobuf`), `StreamDecoder`/`StreamEncoder`,
  datagram helpers (`BF 01 BF` mesh prefix).
* `Transport` and `Connector` traits (`async_trait`, object-safe so
  supervisors can hold `Box<dyn Transport>`).
* `StreamTransport<S>` over any `AsyncRead + AsyncWrite` (TLS will reuse it),
  `TcpConnector`, `TransportConfig` (frame limit, timeouts, idle detection).
* `Endpoint`: `host:port:proto` parsing (`*:-1:stcp`, IPv6 literals).

### `tak-cli`
* `tak cot decode` (summary / JSON / XML; splits captured streams).
* `tak cot encode` (flags → XML; typed details + raw fragments).
* `tak cot validate [--strict]` (exit 1 on failure; strict adds domain mapping
  and lossless round trip).

### `tak-proto`
* Hand-written `prost::Message` structs for TAK Protocol v1 (`TakMessage`,
  `TakControl`, `CotEvent`, `Detail` + typed details); no `protoc` (ADR 0007).
  Reference `.proto` files live in `crates/tak-proto/proto/`.
* `to_proto` / `from_proto` between `tak_cot::CotEvent` and `TakMessage`.
  Typed details that are fully representable are hoisted into protobuf
  fields; everything else (unknown elements, extra attributes, text) travels
  losslessly in `Detail.xmlDetail` as a root-less XML fragment.
* `encode` / `decode` of a single message; stream framing stays in
  `tak-network`.

### `tak-crypto`
* `ClientIdentity` from PEM (`cert`+`key`) or PKCS#12 (`user.p12`), keys in
  `Zeroizing` buffers, `Debug` redacted.
* `TrustStore` from PEM bundles, PKCS#12 trust stores or `webpki-roots`.
* `build_client_config` → `Arc<rustls::ClientConfig>` with explicit
  `Verification::{Full, TrustedChainAnyName, DangerousNoVerify}`.
* `inspect_pem` / `inspect_der` → `CertInfo` (subject, issuer, validity, SANs,
  SHA-256 fingerprint); never includes key material.

### `tak-transport`
* `TlsConnector`: TCP + `tokio-rustls` handshake → `StreamTransport<TlsStream>`;
  implements `tak_network::Connector` so callers do not care about TLS vs TCP.
* `Negotiator`: pure state machine for TAK Protocol negotiation
  (`t-x-takp-v` → `t-x-takp-q` → `t-x-takp-r`); falls back to XML on decline,
  can be pinned to XML.
* `Session`: one connection; drives negotiation, encodes outbound events in
  the negotiated `WireMode`, decodes inbound frames, counts errors instead of
  dropping the connection on one bad frame.
* `Supervisor`: owns the reconnect loop (exponential backoff with equal jitter (`[d/2, d]`),
  `CancellationToken` shutdown), emits `SupervisorEvent`s
  (`Connected`/`WireMode`/`Received`/`Disconnected`/`Stopped`) and takes
  outbound events on a channel. One supervisor per server connection.

### `tak-state`
* `Store`: contacts, objects and chat keyed by UID; `apply(TakEvent)` returns
  the `StoreChange`s it caused so UIs/agents update incrementally.
* No clock of its own: `sweep(now)` removes items past `stale` + grace.
  Capacity limits, chat de-duplication by `message_id`, counters in
  `StoreStats`.
* Depends only on `tak-core` — no XML, protobuf, sockets or async runtime.

### `tak-cli` (phase 2 additions)
* `tak connect` streams events (summary / JSON-lines / XML), optional `--send`.
* `tak contacts`, `tak status` observe a server for N seconds and report.
* `tak cert inspect` for PEM and PKCS#12 (`--password`, `TAK_P12_PASSWORD`).

## Planned crates (see ROADMAP.md)

`tak-storage` (SQLite persistence and replay), UDP mesh SA in
`tak-transport`, `tak-missions` (data packages, mission API), `tak-plugins`
(WASM component sandbox), `tak-ui-core`, `tak-map`.

## Concurrency model

Tokio. Each connection is owned by one task; frames cross task boundaries as
`TakEvent`s over bounded channels. CPU-heavy work (parsing large data
packages, crypto) goes through `spawn_blocking`. Every long-lived task takes a
`CancellationToken` and shutdown is explicit. Panics are bugs: library code is
`#![forbid(unsafe_code)]` with `clippy::unwrap_used`/`expect_used`/`panic`
warned (and denied in CI via `-D warnings`), relaxed only in tests.

## Error handling

Libraries expose typed errors (`ValidationError`, `CotError`, `NetworkError`)
built with `thiserror`. Applications use `anyhow` at the boundary and map to
exit codes. Errors never contain key material; see SECURITY.md.
