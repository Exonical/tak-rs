# Architecture Decision Records

Short, numbered records of decisions that are expensive to reverse. Format:
Context → Decision → Consequences. Supersede rather than edit; mark the old
record "Superseded by NNNN".

| #    | Title                                                        | Status   |
|------|--------------------------------------------------------------|----------|
| 0001 | Rust workspace of small crates, apps as thin shells          | Accepted |
| 0002 | `tak-core` is a wire-neutral canonical domain model          | Accepted |
| 0003 | Hand-written quick-xml CoT codec with lossless detail tree   | Accepted |
| 0004 | Transport trait + frame-level encoding detection             | Accepted |
| 0005 | Offline-first, transport-independent state                   | Accepted |
| 0006 | Security baseline: rustls only, no unsafe, no panics, fuzzed | Accepted |
| 0007 | TAK Protocol via prost with hand-written messages (no protoc)| Accepted |
