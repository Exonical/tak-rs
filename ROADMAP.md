# Roadmap

Priorities, in order: TAK interoperability → cross-platform reuse →
offline-first → security → extensibility → testability → maintainability →
embedded/headless → desktop/mobile UX → minimal platform code.

## Phase 1 — foundation (this PR)

- [x] Cargo workspace, stable toolchain, strict lints, CI (fmt, clippy, test
      on Linux/Windows/macOS, rustdoc, fuzz build, cargo-deny)
- [x] `tak-core` domain model with validating constructors
- [x] `tak-cot` parser/serialiser, typed details, adapters, fixtures,
      proptest, fuzz target
- [x] `tak-network` transport trait, XML + TAK Protocol stream framing, TCP
- [x] `tak-cli cot decode | encode | validate`
- [x] ARCHITECTURE / DESIGN / SECURITY / CONTRIBUTING / ADRs 0001–0007

## Phase 2 — Milestone 1: talk to a TAK Server

- [x] `tak-proto`: TAK Protocol v1 `TakMessage`/`CotEvent`/`Detail` with
      prost (hand-written message structs, no `protoc` build dependency);
      adapters to/from `tak-core`
- [x] `tak-crypto`: PEM and PKCS#12 loading, rustls `ClientConfig` with mTLS,
      trust-store handling, `tak cert inspect`
- [x] `tak-transport`: TLS transport (`StreamTransport<TlsStream>`), TAK
      Protocol negotiation state machine, reconnect supervisor with backoff
      and jitter, graceful shutdown
- [x] `tak-state`: contacts/objects/chat with stale handling, `TakEvent`
      fan-in from multiple transports, change events for UI
- [x] `tak-cli connect [--send] | contacts | status | cert inspect`
- [x] In-process TLS test server covering negotiation, reconnect, mTLS
- [ ] Acceptance against a real TAK Server (needs an endpoint + certs): appears
      as a contact in ATAK/WinTAK/iTAK; sees them; survives connection loss;
      unknown detail does not break parsing
- [ ] Periodic own-position SA from `tak connect` / `tak-agent`

## Phase 3 — headless agent

- [ ] `tak-agent`: config file, systemd unit, GPS (gpsd/NMEA) → SA, sensor
      input plugins, store-and-forward queue when offline
- [ ] UDP mesh SA (multicast 239.2.3.1:6969) and `BF 01 BF` datagrams
- [ ] `tak-storage`: SQLite (sqlx) persistence, offline replay

## Phase 4 — missions and data

- [ ] Data packages (zip + MANIFEST), mission API client (Axum-free; reqwest
      with rustls), file share, enterprise sync basics
- [ ] Map data: MBTiles/GeoJSON/KML import behind `tak-map` traits

## Phase 5 — graphical client

- [ ] `tak-ui-core` view-models over `tak-state`
- [ ] `tak-map` trait + MapLibre Native backend
- [ ] `tak-client` (Dioxus) desktop first, then Android/iOS
- [ ] Plugin host (WASM component model) with capability grants

## Non-goals

Porting ATAK UI pixel-for-pixel; FFmpeg as a core dependency; OpenSSL.
