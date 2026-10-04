//! Reconnecting connection supervisor.
//!
//! Spawns one Tokio task that repeatedly connects, runs a [`Session`], and
//! backs off with jitter on failure. Cancellation is explicit via a
//! [`CancellationToken`]; dropping the handle also stops the task.

use std::sync::Arc;
use std::time::Duration;

use rand::Rng as _;
use tak_core::{TakUid, TransportId};
use tak_cot::CotEvent;
use tak_network::Connector;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::error::TransportError;
use crate::negotiate::WireMode;
use crate::session::{Session, SessionStats};

/// Exponential backoff with full jitter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackoffPolicy {
    /// Delay after the first failure.
    pub initial: Duration,
    /// Upper bound on the delay.
    pub max: Duration,
    /// Growth factor per consecutive failure.
    pub multiplier: f64,
    /// A connection that lasted at least this long resets the backoff.
    pub reset_after: Duration,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
            multiplier: 2.0,
            reset_after: Duration::from_secs(30),
        }
    }
}

impl BackoffPolicy {
    /// Delay for the `attempt`-th consecutive failure (0-based), jittered
    /// uniformly in `[delay / 2, delay]`.
    #[must_use]
    pub fn delay(&self, attempt: u32) -> Duration {
        let base = self.initial.as_secs_f64() * self.multiplier.powi(attempt.min(30) as i32);
        let capped = base.min(self.max.as_secs_f64());
        let jittered = rand::rng().random_range(capped / 2.0..=capped);
        Duration::from_secs_f64(jittered)
    }
}

/// What a supervisor reports.
#[derive(Debug)]
#[non_exhaustive]
pub enum SupervisorEvent {
    /// A connection is up (negotiation may still upgrade the wire mode).
    Connected {
        /// Transport identifier.
        transport: TransportId,
    },
    /// The wire mode changed (XML → protobuf).
    WireMode(WireMode),
    /// An event arrived from the peer.
    Received {
        /// Which connection delivered it.
        transport: TransportId,
        /// The event.
        event: Box<CotEvent>,
    },
    /// A connection attempt failed or a live connection dropped.
    Disconnected {
        /// Why, when known.
        reason: String,
        /// Delay before the next attempt.
        retry_in: Duration,
        /// Stats of the connection that ended, if one was established.
        stats: Option<SessionStats>,
    },
    /// The supervisor was cancelled and has exited.
    Stopped,
}

/// Channels to talk to a running supervisor task.
pub struct SupervisorHandle {
    /// Send events to the peer. Dropped silently while disconnected.
    pub outbound: mpsc::Sender<CotEvent>,
    /// Receive connection state and inbound events.
    pub events: mpsc::Receiver<SupervisorEvent>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl SupervisorHandle {
    /// Request a graceful stop (closes the live session) and wait for the task.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }

    /// Token that stops the supervisor when cancelled.
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }
}

/// Configuration for [`Supervisor::spawn`].
pub struct Supervisor {
    /// How to connect.
    pub connector: Arc<dyn Connector>,
    /// Our UID for negotiation messages.
    pub client_uid: TakUid,
    /// Backoff policy.
    pub backoff: BackoffPolicy,
    /// Stay on XML even if the server offers protobuf.
    pub xml_only: bool,
    /// Capacity of the event and outbound channels.
    pub channel_capacity: usize,
}

impl Supervisor {
    /// Sensible defaults for a connector.
    #[must_use]
    pub fn new(connector: Arc<dyn Connector>, client_uid: TakUid) -> Self {
        Self {
            connector,
            client_uid,
            backoff: BackoffPolicy::default(),
            xml_only: false,
            channel_capacity: 256,
        }
    }

    /// Start the supervisor task on the current runtime.
    #[must_use]
    pub fn spawn(self, cancel: CancellationToken) -> SupervisorHandle {
        let (out_tx, out_rx) = mpsc::channel(self.channel_capacity);
        let (ev_tx, ev_rx) = mpsc::channel(self.channel_capacity);
        let token = cancel.clone();
        let task = tokio::spawn(async move {
            run(self, out_rx, ev_tx, token).await;
        });
        SupervisorHandle {
            outbound: out_tx,
            events: ev_rx,
            cancel,
            task,
        }
    }
}

async fn run(
    sup: Supervisor,
    mut outbound: mpsc::Receiver<CotEvent>,
    events: mpsc::Sender<SupervisorEvent>,
    cancel: CancellationToken,
) {
    let mut attempt: u32 = 0;
    loop {
        if cancel.is_cancelled() {
            break;
        }
        let connected_at = tokio::time::Instant::now();
        let outcome = tokio::select! {
            () = cancel.cancelled() => break,
            r = sup.connector.connect() => r,
        };
        let (reason, stats) = match outcome {
            Err(e) => (format!("connect {}: {e}", sup.connector.describe()), None),
            Ok(transport) => {
                let session = if sup.xml_only {
                    Session::xml_only(transport, sup.client_uid.clone())
                } else {
                    Session::new(transport, sup.client_uid.clone())
                };
                if events
                    .send(SupervisorEvent::Connected {
                        transport: session.id().clone(),
                    })
                    .await
                    .is_err()
                {
                    break;
                }
                match drive(session, &mut outbound, &events, &cancel).await {
                    Driven::Cancelled => break,
                    Driven::Ended { reason, stats } => (reason, Some(stats)),
                }
            }
        };
        if cancel.is_cancelled() {
            break;
        }
        if connected_at.elapsed() >= sup.backoff.reset_after {
            attempt = 0;
        }
        let retry_in = sup.backoff.delay(attempt);
        attempt = attempt.saturating_add(1);
        tracing::warn!(%reason, ?retry_in, "connection lost; reconnecting");
        if events
            .send(SupervisorEvent::Disconnected {
                reason,
                retry_in,
                stats,
            })
            .await
            .is_err()
        {
            break;
        }
        tokio::select! {
            () = cancel.cancelled() => break,
            () = tokio::time::sleep(retry_in) => {}
        }
    }
    let _ = events.send(SupervisorEvent::Stopped).await;
}

enum Driven {
    Cancelled,
    Ended { reason: String, stats: SessionStats },
}

async fn drive(
    mut session: Session,
    outbound: &mut mpsc::Receiver<CotEvent>,
    events: &mpsc::Sender<SupervisorEvent>,
    cancel: &CancellationToken,
) -> Driven {
    let mut mode = session.wire_mode();
    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                let _ = session.close().await;
                return Driven::Cancelled;
            }
            inbound = session.recv() => {
                let result: Result<Option<CotEvent>, TransportError> = inbound;
                match result {
                    Ok(Some(event)) => {
                        if session.wire_mode() != mode {
                            mode = session.wire_mode();
                            if events.send(SupervisorEvent::WireMode(mode)).await.is_err() {
                                return Driven::Cancelled;
                            }
                        }
                        let msg = SupervisorEvent::Received { transport: session.id().clone(), event: Box::new(event) };
                        if events.send(msg).await.is_err() {
                            return Driven::Cancelled;
                        }
                    }
                    Ok(None) => return Driven::Ended { reason: "peer closed connection".into(), stats: session.stats() },
                    Err(e) => return Driven::Ended { reason: e.to_string(), stats: session.stats() },
                }
            }
            to_send = outbound.recv() => {
                match to_send {
                    Some(event) => {
                        if let Err(e) = session.send(&event).await {
                            return Driven::Ended { reason: format!("send failed: {e}"), stats: session.stats() };
                        }
                    }
                    // All senders dropped: nothing more to send, keep receiving.
                    None => std::future::pending::<()>().await,
                }
            }
        }
    }
}
