# Contributing

## Ground rules

* **Layering.** Read ARCHITECTURE.md. `tak-core` stays wire-neutral; codecs
  adapt to it; transports move frames; UI is presentation only. A PR that
  adds `quick_xml` or `prost` to `tak-core` will be declined.
* **No panics in library code.** Return typed errors. `unwrap`/`expect`/
  `panic` are lint-warned and CI runs with `-D warnings`; tests may opt out
  with the crate-level `cfg_attr(test, allow(...))`.
* **Preserve the unknown.** Any detail, attribute or message you do not model
  must survive parse → emit. Add a fixture proving it.
* **Documentation is code.** Public items need rustdoc (`missing_docs` warns).
  Architecture-affecting changes update ARCHITECTURE.md and add an ADR
  (`docs/adr/NNNN-title.md`, copy the template of an existing one).

## Workflow

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features
cargo test --workspace --all-features
cargo doc --workspace --no-deps
```

CI additionally runs `cargo deny check`, builds the fuzz targets, and tests on
Linux, Windows and macOS.

### Adding a CoT fixture

1. Drop the XML under `crates/tak-cot/tests/fixtures/valid/` (or `malformed/`).
2. Add it to the classification table in `crates/tak-cot/tests/fixtures.rs`
   (the test fails until you do).
3. If it exercises a new detail element, add a `KnownDetail` type in
   `crates/tak-cot/src/detail.rs` *only if* the domain model needs it;
   otherwise it is preserved automatically.

### Fuzzing

```sh
cargo install cargo-fuzz
cd crates/tak-cot/fuzz
cargo +nightly fuzz run parse_cot -- -max_len=4096
```

Minimise and commit interesting crashes as malformed fixtures.

## Commit / PR conventions

* Small, coherent commits with imperative subjects.
* PR description: what changed, why, how it was tested, known limitations.
* Do not bump dependency versions and change behaviour in the same PR.

## Licence

By contributing you agree your work is licensed under Apache-2.0.
