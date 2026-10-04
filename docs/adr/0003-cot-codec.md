# ADR 0003 — Hand-written quick-xml CoT codec with lossless detail tree

**Status:** Accepted (2026-10)

## Context
CoT's `<detail>` is open: every TAK client and plugin adds elements. A codec
that drops what it does not know breaks relaying and loses user data.
`serde`-based XML mappings cannot represent arbitrary ordered content without
giving up their advantages, and must not panic on hostile input.

## Decision
`tak-cot` uses `quick-xml` directly:
* `parse` builds `CotEvent` (all event attributes, raw point, detail tree);
  unknown attributes and elements are preserved in order.
* `to_xml` is deterministic; `parse(to_xml(e)) == e` is a tested, fuzzed and
  property-tested invariant.
* Typed views (`KnownDetail`) are *projections* over the tree, each keeping
  unmodelled attributes/children, so modelling a detail never loses data.
* Hardening: DTDs rejected, only predefined/numeric entities, byte/depth/
  node/attribute limits, XML 1.0 character validation on read and write.
* Mixed content is normalised to text-then-children (not used by TAK).

## Consequences
* ~1 kLOC of codec we own and must maintain; offset by full control and
  fuzzability.
* Comments and processing instructions inside `<detail>` are dropped.
* Namespace prefixes are treated as literal name text, matching TAK clients.
