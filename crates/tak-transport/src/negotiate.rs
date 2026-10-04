//! TAK Protocol version negotiation (streaming connections).
//!
//! Sequence on a TAK Server stream:
//! 1. Server announces support: CoT `t-x-takp-v` with
//!    `<TakControl><TakProtocolSupport version="1"/></TakControl>`.
//! 2. Client requests: `t-x-takp-q` with `<TakControl><TakRequest version="1"/></TakControl>`.
//! 3. Server confirms: `t-x-takp-r` with `<TakControl><TakResponse status="true"/></TakControl>`.
//!    From here both sides speak protobuf frames.
//!
//! Until step 3 both sides stay on XML. Servers that never announce support
//! keep the connection on XML indefinitely, which is also fine.
//!
//! This type is a pure state machine: feed it inbound frames, act on the
//! returned outcome. It never performs I/O.

use tak_core::{CotType, DetailNode, Extensions, TakUid, Timestamp};
use tak_cot::{CotEvent, CotPoint};
use tak_network::Frame;

use crate::error::TransportError;

/// Which wire encoding a session currently uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireMode {
    /// CoT XML documents.
    CotXml,
    /// TAK Protocol version 1 protobuf frames.
    TakProtocolV1,
}

/// What the caller must do after feeding a frame to the negotiator.
#[derive(Debug, PartialEq, Eq)]
pub enum NegotiatorOutcome {
    /// Not a negotiation frame; deliver it to the application.
    PassThrough,
    /// Negotiation frame consumed; nothing to send.
    Consumed,
    /// Negotiation frame consumed; send this reply (still in XML).
    Reply(Frame),
    /// Negotiation complete; switch encoding for all subsequent traffic.
    Switch(WireMode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Xml,
    Requested,
    Protobuf,
}

/// Negotiation state for one connection.
#[derive(Debug)]
pub struct Negotiator {
    state: State,
    client_uid: TakUid,
    /// Whether the client wants protobuf at all.
    want_proto: bool,
}

/// CoT type of the server's support announcement.
pub const TYPE_SUPPORT: &str = "t-x-takp-v";
/// CoT type of the client's request.
pub const TYPE_REQUEST: &str = "t-x-takp-q";
/// CoT type of the server's response.
pub const TYPE_RESPONSE: &str = "t-x-takp-r";

impl Negotiator {
    /// A negotiator that will upgrade to protobuf when offered.
    #[must_use]
    pub fn new(client_uid: TakUid) -> Self {
        Self {
            state: State::Xml,
            client_uid,
            want_proto: true,
        }
    }

    /// A negotiator that stays on XML (useful for debugging/interop tests).
    #[must_use]
    pub fn xml_only(client_uid: TakUid) -> Self {
        Self {
            want_proto: false,
            ..Self::new(client_uid)
        }
    }

    /// Current encoding.
    #[must_use]
    pub fn mode(&self) -> WireMode {
        match self.state {
            State::Xml | State::Requested => WireMode::CotXml,
            State::Protobuf => WireMode::TakProtocolV1,
        }
    }

    /// Inspect an inbound frame. Only XML frames can be negotiation
    /// messages; protobuf frames always pass through.
    pub fn on_frame(
        &mut self,
        frame: &Frame,
        now: Timestamp,
    ) -> Result<NegotiatorOutcome, TransportError> {
        let Some(xml) = frame.as_xml_str() else {
            return Ok(NegotiatorOutcome::PassThrough);
        };
        // Cheap pre-check before a full parse.
        if !xml.contains("t-x-takp-") {
            return Ok(NegotiatorOutcome::PassThrough);
        }
        let Ok(event) = tak_cot::parse(xml) else {
            return Ok(NegotiatorOutcome::PassThrough);
        };
        match event.cot_type.as_str() {
            TYPE_SUPPORT => {
                if !self.want_proto || self.state != State::Xml {
                    return Ok(NegotiatorOutcome::Consumed);
                }
                let supported = control_versions(&event.detail, "TakProtocolSupport");
                if !supported.contains(&tak_proto::TAK_PROTO_VERSION) {
                    tracing::info!(
                        ?supported,
                        "server offers no TAK Protocol version we speak; staying on XML"
                    );
                    return Ok(NegotiatorOutcome::Consumed);
                }
                self.state = State::Requested;
                let request = control_event(
                    &self.client_uid,
                    TYPE_REQUEST,
                    now,
                    "TakRequest",
                    &[("version", "1")],
                )?;
                let xml =
                    tak_cot::to_xml(&request).map_err(|e| TransportError::Encode(e.to_string()))?;
                Ok(NegotiatorOutcome::Reply(Frame::xml(xml)))
            }
            TYPE_RESPONSE => {
                if self.state != State::Requested {
                    return Ok(NegotiatorOutcome::Consumed);
                }
                let ok = event
                    .detail
                    .get("TakControl")
                    .and_then(|c| c.child("TakResponse"))
                    .and_then(|r| r.attr("status"))
                    .is_some_and(|s| s.eq_ignore_ascii_case("true"));
                if ok {
                    self.state = State::Protobuf;
                    tracing::info!("TAK Protocol v1 negotiated");
                    Ok(NegotiatorOutcome::Switch(WireMode::TakProtocolV1))
                } else {
                    self.state = State::Xml;
                    self.want_proto = false;
                    tracing::warn!("server declined TAK Protocol; staying on XML");
                    Ok(NegotiatorOutcome::Consumed)
                }
            }
            TYPE_REQUEST => Ok(NegotiatorOutcome::Consumed),
            _ => Ok(NegotiatorOutcome::PassThrough),
        }
    }
}

fn control_versions(detail: &Extensions, element: &str) -> Vec<u32> {
    detail
        .get("TakControl")
        .map(|c| {
            c.children_named(element)
                .filter_map(|n| n.attr("version"))
                .filter_map(|v| v.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Build a `TakControl` CoT event (used for requests; exposed for tests and
/// in-process servers).
pub fn control_event(
    uid: &TakUid,
    cot_type: &str,
    now: Timestamp,
    element: &str,
    attrs: &[(&str, &str)],
) -> Result<CotEvent, TransportError> {
    let mut inner = DetailNode::new(element);
    for (k, v) in attrs {
        inner.set_attr(*k, *v);
    }
    let detail = Extensions::from_nodes(vec![DetailNode::new("TakControl").with_child(inner)]);
    let stale = Timestamp::from_unix_millis(now.unix_millis().saturating_add(60_000))
        .map_err(|e| TransportError::Encode(e.to_string()))?;
    let cot_type = CotType::new(cot_type).map_err(|e| TransportError::Encode(e.to_string()))?;
    Ok(CotEvent::new(
        uid.clone(),
        cot_type,
        CotPoint {
            lat: 0.0,
            lon: 0.0,
            hae: 0.0,
            ce: 999_999.0,
            le: 999_999.0,
        },
        now,
        stale,
    )
    .with_detail(detail))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn uid() -> TakUid {
        TakUid::new("CLIENT-1").unwrap()
    }

    fn server(cot_type: &str, element: &str, attrs: &[(&str, &str)]) -> Frame {
        let ev = control_event(
            &TakUid::new("SERVER").unwrap(),
            cot_type,
            Timestamp::now(),
            element,
            attrs,
        )
        .unwrap();
        Frame::xml(tak_cot::to_xml(&ev).unwrap())
    }

    #[test]
    fn full_handshake_switches_to_protobuf() {
        let mut n = Negotiator::new(uid());
        let now = Timestamp::now();
        let out = n
            .on_frame(
                &server(TYPE_SUPPORT, "TakProtocolSupport", &[("version", "1")]),
                now,
            )
            .unwrap();
        let NegotiatorOutcome::Reply(reply) = out else {
            panic!("expected reply, got {out:?}")
        };
        let req = tak_cot::parse(reply.as_xml_str().unwrap()).unwrap();
        assert_eq!(req.cot_type.as_str(), TYPE_REQUEST);
        assert_eq!(req.uid.as_str(), "CLIENT-1");
        assert_eq!(n.mode(), WireMode::CotXml);
        let out = n
            .on_frame(
                &server(TYPE_RESPONSE, "TakResponse", &[("status", "true")]),
                now,
            )
            .unwrap();
        assert_eq!(out, NegotiatorOutcome::Switch(WireMode::TakProtocolV1));
        assert_eq!(n.mode(), WireMode::TakProtocolV1);
    }

    #[test]
    fn declined_response_stays_xml() {
        let mut n = Negotiator::new(uid());
        let now = Timestamp::now();
        n.on_frame(
            &server(TYPE_SUPPORT, "TakProtocolSupport", &[("version", "1")]),
            now,
        )
        .unwrap();
        let out = n
            .on_frame(
                &server(TYPE_RESPONSE, "TakResponse", &[("status", "false")]),
                now,
            )
            .unwrap();
        assert_eq!(out, NegotiatorOutcome::Consumed);
        assert_eq!(n.mode(), WireMode::CotXml);
    }

    #[test]
    fn unknown_version_and_xml_only_do_not_request() {
        let now = Timestamp::now();
        let mut n = Negotiator::new(uid());
        let out = n
            .on_frame(
                &server(TYPE_SUPPORT, "TakProtocolSupport", &[("version", "7")]),
                now,
            )
            .unwrap();
        assert_eq!(out, NegotiatorOutcome::Consumed);
        let mut n = Negotiator::xml_only(uid());
        let out = n
            .on_frame(
                &server(TYPE_SUPPORT, "TakProtocolSupport", &[("version", "1")]),
                now,
            )
            .unwrap();
        assert_eq!(out, NegotiatorOutcome::Consumed);
        assert_eq!(n.mode(), WireMode::CotXml);
    }

    #[test]
    fn unsolicited_response_and_ordinary_events_pass_through() {
        let now = Timestamp::now();
        let mut n = Negotiator::new(uid());
        let out = n
            .on_frame(
                &server(TYPE_RESPONSE, "TakResponse", &[("status", "true")]),
                now,
            )
            .unwrap();
        assert_eq!(out, NegotiatorOutcome::Consumed);
        assert_eq!(n.mode(), WireMode::CotXml);
        let sa = Frame::xml(
            r#"<event version="2.0" uid="X" type="a-f-G" time="2024-05-01T12:00:00Z" start="2024-05-01T12:00:00Z" stale="2024-05-01T12:05:00Z"><point lat="1" lon="2" hae="0" ce="1" le="1"/></event>"#,
        );
        assert_eq!(
            n.on_frame(&sa, now).unwrap(),
            NegotiatorOutcome::PassThrough
        );
        assert_eq!(
            n.on_frame(&Frame::TakProtobuf(bytes::Bytes::new()), now)
                .unwrap(),
            NegotiatorOutcome::PassThrough
        );
    }
}
