# ADR 0009 — State store is synchronous and clock-free

**Status:** Accepted (2026-10)

## Context
`tak-state` is consumed by a Dioxus UI, a headless agent and tests. Hidden
timers and background tasks make staleness non-deterministic and tie the
crate to an async runtime.

## Decision
`Store` is a plain synchronous value: `apply(TakEvent) -> Vec<StoreChange>`
and `sweep(now) -> Vec<StoreChange>`. The caller owns the clock and the
schedule (a UI ticks every second; an agent may sweep every minute; tests
pass fixed timestamps). Stale items stay visible until `stale + grace`
so a UI can render "stale" before "gone". Capacity limits reject rather
than evict live data; chat is de-duplicated by `message_id`.

## Consequences
* Deterministic tests; no `tokio` dependency in `tak-state`.
* Reactive wrappers (watch channels, Dioxus signals, persistence) are built
  on top of `StoreChange`, not inside the store.
* Replayed or out-of-order reports are last-write-wins for now; ordering by
  `time` is a possible follow-up.
