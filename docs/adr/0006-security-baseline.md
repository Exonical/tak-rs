# ADR 0006 — Security baseline: rustls only, no unsafe, no panics, fuzzed

**Status:** Accepted (2026-10)

## Context
TAK-RS parses untrusted network input and will hold client certificates and
private keys. It must build on platforms where OpenSSL is painful (Android,
iOS, WASM, cross-compiled ARM).

## Decision
* TLS: `rustls` exclusively; OpenSSL/native-tls crates banned in `deny.toml`.
* `#![forbid(unsafe_code)]` workspace-wide; no FFI in core crates.
* `unwrap`/`expect`/`panic` lint-warned, CI `-D warnings`; tests opt out.
* Every parser gets explicit resource limits, a fuzz target and property
  tests; malformed fixtures are committed.
* Private keys live in zeroising buffers; no `Debug`, no logs, no serde.
* `cargo-deny` (advisories, licences, bans, sources) runs in CI; `--locked`
  builds.

## Consequences
* Some platform-specific TLS features (e.g. OS keychains) need separate,
  optional integration later.
* Slightly slower iteration due to strict lints; offset by fewer runtime
  failures in the field.
