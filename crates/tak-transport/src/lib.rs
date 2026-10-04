//! Connecting to TAK servers: TLS, TAK Protocol negotiation, and a
//! reconnecting session supervisor.
//!
//! Layers (bottom-up):
//! * [`TlsConnector`] — `tak-network`'s [`tak_network::StreamTransport`] over
//!   `tokio-rustls`, configured by `tak-crypto`.
//! * [`Negotiator`] — pure state machine for the TAK Protocol version
//!   handshake (`t-x-takp-v` → `t-x-takp-q` → `t-x-takp-r`).
//! * [`Session`] — one live connection speaking CoT events (XML or protobuf
//!   chosen by negotiation) over any [`tak_network::Transport`].
//! * [`Supervisor`] — owns a [`tak_network::Connector`], reconnects with
//!   jittered exponential backoff, and exposes channels of
//!   [`SupervisorEvent`]s. Stops cleanly via a
//!   [`tokio_util::sync::CancellationToken`].
//!
//! Nothing here depends on how state is stored or displayed.

pub mod error;
pub mod negotiate;
pub mod session;
pub mod supervisor;
pub mod tls;

pub use error::TransportError;
pub use negotiate::{Negotiator, NegotiatorOutcome, WireMode};
pub use session::{Inbound, Session, SessionStats};
pub use supervisor::{BackoffPolicy, Supervisor, SupervisorEvent, SupervisorHandle};
pub use tls::TlsConnector;
