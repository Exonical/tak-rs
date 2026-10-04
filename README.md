# TAK-RS

A modern, cross-platform implementation of the TAK ecosystem in Rust.

TAK-RS is **not** a line-by-line port of ATAK. It is a clean, modular,
TAK-compatible platform built as a reusable Rust SDK with thin application
shells on top:

| Application  | Purpose                                                    | Status      |
|--------------|------------------------------------------------------------|-------------|
| `tak-cli`    | CLI codec tools and debugging harness                      | Phase 1     |
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
apps/
  tak-cli       `tak cot decode | encode | validate`
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
