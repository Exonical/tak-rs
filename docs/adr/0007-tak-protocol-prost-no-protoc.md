# ADR 0007 — TAK Protocol via prost with hand-written messages (no protoc)

**Status:** Accepted (2026-10) — implementation in phase 2

## Context
TAK Protocol v1 is a small, stable protobuf schema (`TakMessage`,
`TakControl`, `CotEvent`, `Detail`, `Contact`, `Group`, `Status`, `Takv`,
`Track`, `PrecisionLocation`). `prost-build` requires `protoc` at build time,
which complicates cross-compilation, mobile toolchains and offline builds.

## Decision
`tak-proto` uses `prost` runtime derives (`#[derive(prost::Message)]`) on
hand-written structs mirroring the official `.proto` files, which are vendored
under `tak-proto/proto/` for reference and a CI check that compares the
hand-written field numbers against the schema when `protoc` *is* available.
Unknown fields are not preserved by prost; the XML `<detail>` string field
inside `Detail.xmlDetail` is parsed with `tak-cot`'s detail parser so unknown
detail content stays lossless.

## Consequences
* No build-time external tool; `cargo build` works everywhere.
* Schema drift must be caught by the optional CI comparison and fixtures.
* Protobuf and XML paths share one detail model.
