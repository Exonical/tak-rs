# Design notes

Rationale and trade-offs behind the phase-1 code. Decisions with lasting
consequences are also recorded as ADRs in `docs/adr`.

## Two representations of a CoT event

`tak_cot::CotEvent` is deliberately *not* the domain model. It is the wire
shape: raw `f64` point fields with the `9999999` sentinel, `version`, every
event attribute, and the detail tree. Keeping it separate buys:

* **Losslessness** — the serialiser can reproduce exactly what was parsed,
  which matters when relaying traffic for clients that use details we do not
  model.
* **A stable domain** — `tak-core` can describe a contact's speed as
  `Option<Speed>` rather than "the `speed` attribute of the `track` element if
  present and numeric".
* **Fuzzability** — `parse ∘ to_xml` is an identity on `CotEvent`, which is
  a cheap, strong invariant to fuzz and property-test.

The adapter then classifies a `CotEvent` (contact / object / chat / delete /
control) and converts. Consumed detail elements (`contact`, `__group`,
`takv`, `track`, …) become typed fields; everything else is kept in
`Extensions` on the resulting entity, and `from_*` re-emits them.

## Unknown detail preservation

`tak_core::DetailNode { name, attributes, text, children }` is the smallest
tree that can round-trip arbitrary XML element content without being XML:
attribute order and child order are preserved as `Vec`s; comments, processing
instructions and namespace semantics are not (namespaced names are kept as the
literal `prefix:local` string, which is what TAK clients compare anyway).
Mixed content is reduced to "all text concatenated, then children"; TAK
details never use interleaved mixed content in practice.

## Parser hardening

`ParseLimits` bounds bytes (1 MiB), depth (32), element count (10 000) and
attributes per element (256). DTDs are rejected outright (no entity expansion
attacks, no external entities); only the five predefined and numeric character
references are expanded. Illegal XML 1.0 characters are rejected on both
parse and write so a hostile remark cannot produce an unparseable document for
the next hop. The fuzz target (`crates/tak-cot/fuzz`) checks that nothing
panics and that anything accepted round-trips.

## Stream framing

TAK streaming connections start as concatenated XML documents and, after
`TakProtocolSupport`/`TakRequest`/`TakResponse` negotiation, switch to
`0xBF <varint len> <protobuf>` frames. `StreamDecoder` decides per frame on the
first byte (`<` or BOM ⇒ XML, `0xBF` ⇒ protobuf), so the switch is handled
without the transport knowing about negotiation. XML frames end at the first
`</event>`; escaped text cannot contain that sequence, so this is exact for
well-formed documents. Oversized frames fail fast — the declared protobuf
length is checked before buffering, and unterminated XML is rejected once it
exceeds the limit.

## Transport abstraction

`Transport` is object-safe (`async_trait`) so a supervisor can hold
`Box<dyn Transport>` from any `Connector` and swap TCP/TLS/QUIC without
generics leaking into `tak-state`. `StreamTransport<S>` is generic over the
byte stream so `tokio_rustls::client::TlsStream<TcpStream>` drops straight in.
Reads are capped per call and idle-timed so a stalled or hostile peer surfaces
as `NetworkError::Timeout` rather than a hung task.

## Validation everywhere, panics nowhere

Every `tak-core` constructor validates (`GeoPoint::from_degrees`,
`TakUid::new`, `CotType::new`, …) and returns `ValidationError`. Infallible
fallbacks exist only where the type system forces a value (`GeoPoint::ORIGIN`
for an empty geometry). Numeric parsing in `tak-cot` is tolerant of the
integer sentinels WinTAK emits (`hae="9999999"`) and the two-digit fractional
seconds some servers send.

## Why not `serde` for XML?

CoT's detail subtree is open-ended and order-sensitive; `serde`-driven XML
either loses unknown content or requires `Value`-like escape hatches that give
up its benefits. A small hand-written quick-xml reader/writer is ~600 lines,
fully controlled, and fuzzable. `serde` is used where it fits: optional
derives on `tak-core` for JSON/SQLite, and `tak-cli --format json`.

## Testing strategy

* Unit tests next to the code for each invariant.
* **Fixtures** (`crates/tak-cot/tests/fixtures`): one file per real-world
  message shape (ATAK/WinTAK/iTAK SA, GeoChat broadcast/direct, hostile
  marker, circle/polygon/rectangle, route, delete, ping, protocol
  negotiation, 911 alert, UAS sensor/video, vendor extensions) plus a
  malformed set (truncation, DTD/XXE/billion-laughs, unknown entity, bad
  coordinates, missing fields, wrong root, trailing documents). Every valid
  fixture must parse, round-trip (wire *and* pretty), adapt to a `TakEvent`,
  and re-adapt identically after re-encoding; the classification table must
  list every fixture.
* **Property tests** (`proptest`): generated events with random detail trees
  round-trip; arbitrary and mutated inputs never panic.
* **Fuzzing** (`cargo-fuzz`): same invariants with coverage guidance.
* **Loopback TCP** tests exercise real sockets, split frames, oversize
  frames, idle timeouts and half-closed peers.
* **Black-box CLI** tests run the `tak` binary against the fixtures.
