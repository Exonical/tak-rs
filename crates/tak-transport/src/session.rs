//! A [`Session`]: one connection exchanging [`CotEvent`]s.

use tak_core::{TakUid, Timestamp, TransportId};
use tak_cot::CotEvent;
use tak_network::{Frame, Transport};

use crate::error::TransportError;
use crate::negotiate::{Negotiator, NegotiatorOutcome, WireMode};

/// Something the peer sent that the application should know about.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Inbound {
    /// An application event.
    Event(Box<CotEvent>),
    /// Negotiation finished; subsequent traffic uses this encoding.
    WireMode(WireMode),
}

/// Counters for diagnostics (`tak status`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SessionStats {
    /// Frames received from the peer (including negotiation).
    pub frames_in: u64,
    /// Frames sent to the peer (including negotiation).
    pub frames_out: u64,
    /// Inbound frames that failed to decode and were skipped.
    pub decode_errors: u64,
    /// Events delivered to the application.
    pub events_in: u64,
    /// Events sent by the application.
    pub events_out: u64,
}

/// Wraps a [`Transport`] and speaks CoT events in whichever encoding the
/// peer negotiates.
///
/// Inbound frames that fail to decode are logged, counted in
/// [`SessionStats::decode_errors`] and skipped; a single malformed message
/// from a peer should not tear down the connection.
pub struct Session {
    transport: Box<dyn Transport>,
    negotiator: Negotiator,
    stats: SessionStats,
}

impl Session {
    /// Start a session on a connected transport. `client_uid` identifies us in
    /// negotiation messages.
    #[must_use]
    pub fn new(transport: Box<dyn Transport>, client_uid: TakUid) -> Self {
        Self {
            transport,
            negotiator: Negotiator::new(client_uid),
            stats: SessionStats::default(),
        }
    }

    /// Like [`Session::new`] but never upgrades to protobuf.
    #[must_use]
    pub fn xml_only(transport: Box<dyn Transport>, client_uid: TakUid) -> Self {
        Self {
            transport,
            negotiator: Negotiator::xml_only(client_uid),
            stats: SessionStats::default(),
        }
    }

    /// Transport identifier.
    #[must_use]
    pub fn id(&self) -> &TransportId {
        self.transport.id()
    }

    /// Current wire encoding.
    #[must_use]
    pub fn wire_mode(&self) -> WireMode {
        self.negotiator.mode()
    }

    /// Diagnostics.
    #[must_use]
    pub fn stats(&self) -> SessionStats {
        self.stats
    }

    /// Encode and send one event in the current wire mode.
    pub async fn send(&mut self, event: &CotEvent) -> Result<(), TransportError> {
        let frame = match self.negotiator.mode() {
            WireMode::CotXml => Frame::xml(
                tak_cot::to_xml(event).map_err(|e| TransportError::Encode(e.to_string()))?,
            ),
            WireMode::TakProtocolV1 => {
                let msg = tak_proto::to_proto(event)
                    .map_err(|e| TransportError::Encode(e.to_string()))?;
                Frame::TakProtobuf(tak_proto::encode(&msg))
            }
        };
        self.send_frame(frame).await?;
        self.stats.events_out += 1;
        Ok(())
    }

    /// Receive the next application event or wire-mode change, handling
    /// negotiation replies internally. `Ok(None)` means the peer closed the
    /// connection.
    pub async fn recv(&mut self) -> Result<Option<Inbound>, TransportError> {
        loop {
            let Some(frame) = self.transport.recv().await? else {
                return Ok(None);
            };
            self.stats.frames_in += 1;
            match self.negotiator.on_frame(&frame, Timestamp::now())? {
                NegotiatorOutcome::PassThrough => {}
                NegotiatorOutcome::Consumed => continue,
                NegotiatorOutcome::Switch(mode) => return Ok(Some(Inbound::WireMode(mode))),
                NegotiatorOutcome::Reply(reply) => {
                    self.send_frame(reply).await?;
                    continue;
                }
            }
            match decode(&frame) {
                Ok(Some(event)) => {
                    self.stats.events_in += 1;
                    return Ok(Some(Inbound::Event(Box::new(event))));
                }
                Ok(None) => {}
                Err(e) => {
                    self.stats.decode_errors += 1;
                    tracing::warn!(error = %e, kind = ?frame.kind(), len = frame.len(), "skipping undecodable frame");
                }
            }
        }
    }

    /// Close the connection gracefully.
    pub async fn close(&mut self) -> Result<(), TransportError> {
        Ok(self.transport.close().await?)
    }

    async fn send_frame(&mut self, frame: Frame) -> Result<(), TransportError> {
        self.transport.send(frame).await?;
        self.stats.frames_out += 1;
        Ok(())
    }
}

/// Decode a frame into an event. `Ok(None)` for protobuf control-only
/// messages (mesh negotiation), which carry no event.
fn decode(frame: &Frame) -> Result<Option<CotEvent>, String> {
    match frame {
        Frame::CotXml(_) => {
            let xml = frame
                .as_xml_str()
                .ok_or_else(|| "XML frame is not UTF-8".to_owned())?;
            tak_cot::parse(xml).map(Some).map_err(|e| e.to_string())
        }
        Frame::TakProtobuf(bytes) => {
            let msg = tak_proto::decode(bytes).map_err(|e| e.to_string())?;
            if msg.cot_event.is_none() {
                return Ok(None);
            }
            tak_proto::from_proto(&msg)
                .map(Some)
                .map_err(|e| e.to_string())
        }
    }
}
