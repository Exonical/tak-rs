# TAK-RS

A modern, cross-platform implementation of the TAK ecosystem in Rust.

TAK-RS is **not** a line-by-line port of ATAK. It is a clean, modular,
TAK-compatible platform built as a reusable Rust SDK with thin application
shells on top:

| Application  | Purpose                                                    | Status      |
|--------------|------------------------------------------------------------|-------------|
| `tak-cli`    | CLI codec tools, server connectivity and debugging harness | Phase 1–2   |
| `tak-agent`  | Headless Raspberry Pi / vehicle / sensor-gateway node      | planned     |
| `tak-client` | Dioxus map client for desktop and mobile                   | planned     |

Interoperates (or will) with TAK Server, ATAK, iTAK, WinTAK, Cursor-on-Target
XML, TAK Protocol (protobuf), TAK data packages and TLS certificate
authentication. See [ROADMAP.md](ROADMAP.md) for what exists today.

## Crates

```
crates/
  tak-core      canonical, wire-neutral domain model (no XML/protobuf/UI types)
  tak-cot       Cursor-on-Target XML codec; unknown <detail> content is preserved
  tak-network   Transport trait, CoT-XML + TAK Protocol stream framing, TCP
  tak-proto     TAK Protocol v1 (protobuf) via prost, hand-written, no protoc
  tak-crypto    PEM/PKCS#12 identities, trust stores, rustls mTLS config, cert inspect
  tak-transport TLS connector, TAK Protocol negotiation, session, reconnect supervisor
  tak-state     Transport-independent contacts/objects/chat store with change events
apps/
  tak-cli       `tak cot …`, `tak connect | contacts | status`, `tak cert inspect`
```

Read [ARCHITECTURE.md](ARCHITECTURE.md) for the layering rules and
[docs/adr](docs/adr) for the decisions behind them.

## Quick start

```sh
cargo build --workspace
cargo test --workspace

# decode a captured stream (several events, as from `nc host 8087 > cap.xml`)
tak cot decode cap.xml
tak cot decode --format json --domain cap.xml

# build a position report
tak cot encode --uid ME --lat 38.9 --lon=-77.0 --callsign ALPHA --team Cyan --role "Team Lead"

# CI-style validation (exit 1 on any failure)
tak cot validate --strict crates/tak-cot/tests/fixtures/valid/*.xml

# plain TCP (port 8087): stream everything the server relays, as JSON lines
tak connect --server takserver.example:8087:tcp --format jsonl

# mTLS (port 8089) with the files TAK Server hands out; negotiates TAK Protocol
export TAK_P12_PASSWORD=atakatak
tak status   --server takserver.example:8089:ssl --p12 user.p12 --truststore truststore-root.p12
tak contacts --server 10.0.0.5:8089:ssl --p12 user.p12 --truststore truststore-root.p12 --no-verify-hostname

# what is in that certificate? (never prints key material)
tak cert inspect user.p12
```

## Development

Stable Rust (`rust-toolchain.toml`). Before pushing:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features   # CI runs with -D warnings
cargo test --workspace --all-features
cargo doc --workspace --no-deps
```

Fuzzing (nightly): `cd crates/tak-cot/fuzz && cargo +nightly fuzz run parse_cot`.

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).

## License

Apache-2.0. TAK, ATAK, WinTAK and iTAK are trademarks of their respective
owners; TAK-RS is an independent project.
