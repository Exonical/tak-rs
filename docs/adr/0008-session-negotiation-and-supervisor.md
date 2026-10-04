# ADR 0008 — Session owns negotiation; Supervisor owns reconnection

**Status:** Accepted (2026-10)

## Context
A TAK Server connection has three concerns that are easy to tangle: the byte
transport (TCP/TLS), the TAK Protocol negotiation that switches a live stream
from CoT XML to protobuf, and the reconnect policy. ATAK's `commoncommo` mixes
these in one C++ class. We want each to be testable without sockets and
reusable by `tak-cli`, `tak-agent` and `tak-client`.

## Decision
* `tak-network::Transport` moves `Frame`s and knows nothing about CoT.
* `tak-transport::Negotiator` is a pure state machine: `on_frame(frame, now)`
  returns `PassThrough | Consumed | Reply(frame) | Switch(mode)`. It accepts
  only a server `t-x-takp-r` that answers *our* `t-x-takp-q`, falls back to
  XML on `status="false"` or an unsupported version, and can be pinned to XML.
* `Session` wraps one `Box<dyn Transport>` + one `Negotiator`, encodes
  outbound `CotEvent`s in the current `WireMode`, decodes inbound frames and
  counts (rather than fails on) undecodable ones.
* `Supervisor` wraps a `Connector` and runs the connect → session → backoff
  loop. Full-jitter exponential backoff resets after a stable connection.
  Shutdown is a `CancellationToken`; it tries a graceful `close()` first.
  Communication with the application is two channels: outbound `CotEvent`s
  in, `SupervisorEvent`s out.

## Consequences
* Negotiation is unit-tested with fixtures, no runtime needed.
* TLS vs TCP is invisible above `Connector`; `tak connect --server
  host:8087:tcp` and `host:8089:ssl` share every line of session code.
* One `Supervisor` per server; multi-server fan-in is the application's job
  (feed several supervisors into one `tak-state::Store`).
* The in-process test server in `crates/tak-transport/tests` is the
  interoperability proxy until a real TAK Server is available; real-server
  acceptance remains an open roadmap item.
