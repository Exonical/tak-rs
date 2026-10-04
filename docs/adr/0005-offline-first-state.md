# ADR 0005 — Offline-first, transport-independent state

**Status:** Accepted (2026-10)

## Context
Field devices lose connectivity routinely. Many TAK features degrade badly
when the server is unreachable. Embedded agents may have no server at all
(mesh only) or must queue data for later.

## Decision
State (`tak-state`, phase 2) is fed exclusively by `TakEvent` values and never
holds a socket. "Disconnected" is a normal state with full local function:
own position, local markers, chat drafts and queued outbound events persist.
Transports are supervised separately and push events in; outbound events go
through a store-and-forward queue. Persistence (`tak-storage`, SQLite via
`sqlx`) is behind a trait so headless and WASM builds can swap backends.

## Consequences
* Every feature is designed for "server may never answer".
* Replay/testing is trivial: feed recorded `TakEvent`s.
* Conflict handling (same UID from two transports) is explicit in state, not
  implicit in socket order.
