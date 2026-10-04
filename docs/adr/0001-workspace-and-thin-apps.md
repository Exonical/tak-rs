# ADR 0001 — Rust workspace of small crates; applications are thin shells

**Status:** Accepted (2026-10)

## Context
TAK-RS must run as a CLI, a headless agent, and a desktop/mobile GUI, on
x86-64 and ARM Linux, Windows, macOS, Android and iOS, with WASM a possibility.
Business logic duplicated per application would diverge immediately.

## Decision
One Cargo workspace (`resolver = "3"`, edition 2024, stable toolchain pinned
via `rust-toolchain.toml`). Library crates under `crates/` each own one
concern (`tak-core`, `tak-cot`, `tak-network`, …). Applications under `apps/`
contain argument parsing, configuration, logging setup and UI only; every
operational decision lives in a library crate and is testable without the app.
Crates are created when they have real code — no empty placeholders.
Workspace-level lints: `forbid(unsafe_code)`, `missing_docs`,
`unreachable_pub`, `clippy::pedantic`, `unwrap_used`/`expect_used`/`panic`.

## Consequences
* Features ship first in a crate + `tak-cli`, then surface in GUI.
* Mobile builds can exclude heavy crates with features.
* More crates means more `Cargo.toml` upkeep; workspace dependency
  inheritance keeps versions in one place.
