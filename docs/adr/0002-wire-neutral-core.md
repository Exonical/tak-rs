# ADR 0002 — `tak-core` is a wire-neutral canonical domain model

**Status:** Accepted (2026-10)

## Context
TAK speaks CoT XML, TAK Protocol protobuf, and REST/JSON for missions. ATAK's
internal model is effectively "the XML", which makes every consumer an XML
consumer. We want state, storage, UI and plugins to be independent of the
wire format in use.

## Decision
`tak-core` defines the canonical types (`TakUid`, `GeoPoint`, `CotType`,
`Contact`, `TakObject`, `ChatMessage`, `TakEvent`, …). It depends on no XML,
protobuf, UI, map or database crate. Codecs (`tak-cot`, `tak-proto`) convert
to and from these types. Constructors validate and return `ValidationError`.
Unknown wire content is carried as `Extensions` (an ordered tree of
`DetailNode`) — a format-neutral representation, not XML.
`TakEvent` is `#[non_exhaustive]`.

## Consequences
* Two representations of a CoT event exist (`tak_cot::CotEvent` wire shape
  and `tak-core` entities). The adapter layer is explicit, tested and fuzzed.
* Some CoT concepts (e.g. `9999999` sentinels) become `Option`s in the core.
* Adding a wire format never touches state or UI.
