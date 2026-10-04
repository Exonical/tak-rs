//! Canonical events produced by protocol adapters and consumed by the state engine.

use crate::chat::ChatMessage;
use crate::contact::Contact;
use crate::cot_type::CotType;
use crate::object::TakObject;
use crate::source::ObjectSource;
use crate::time::Timestamp;
use crate::uid::TakUid;

/// Something that happened in the TAK world, independent of wire format.
///
/// Both the CoT XML adapter and the TAK protobuf adapter produce exactly this
/// type so that the state engine, storage and UI never see protocol details.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "event", rename_all = "snake_case"))]
#[non_exhaustive]
pub enum TakEvent {
    /// A position/presence report from another TAK user.
    ContactUpdated(Contact),
    /// A map object was created or updated.
    ObjectUpdated(TakObject),
    /// An object was explicitly deleted by its owner (`t-x-d-d`).
    ObjectRemoved {
        /// UID of the removed object.
        uid: TakUid,
        /// When the deletion was issued.
        at: Timestamp,
        /// Who told us.
        source: ObjectSource,
    },
    /// A chat message was received.
    ChatReceived(ChatMessage),
    /// A control / tasking message that carries no map state.
    Control {
        /// Classification of the control message.
        kind: ControlKind,
        /// Full CoT type for diagnostics.
        cot_type: CotType,
        /// UID of the control event.
        uid: TakUid,
        /// Who sent it.
        source: ObjectSource,
    },
}

impl TakEvent {
    /// UID of the entity this event is about.
    pub fn uid(&self) -> &TakUid {
        match self {
            Self::ContactUpdated(c) => &c.uid,
            Self::ObjectUpdated(o) => &o.uid,
            Self::ObjectRemoved { uid, .. } | Self::Control { uid, .. } => uid,
            Self::ChatReceived(m) => &m.message_id,
        }
    }

    /// Provenance of this event.
    pub fn source(&self) -> &ObjectSource {
        match self {
            Self::ContactUpdated(c) => &c.source,
            Self::ObjectUpdated(o) => &o.source,
            Self::ObjectRemoved { source, .. } | Self::Control { source, .. } => source,
            Self::ChatReceived(_) => &ObjectSource::LOCAL,
        }
    }
}

/// Classification of control messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ControlKind {
    /// Keep-alive ping (`t-x-c-t`).
    Ping,
    /// Keep-alive response (`t-x-c-t-r`).
    Pong,
    /// TAK protocol version negotiation (`t-x-takp-*`).
    ProtocolNegotiation,
    /// Any other tasking message.
    Other,
}

impl ControlKind {
    /// Classify a tasking CoT type.
    pub fn from_cot_type(t: &CotType) -> Self {
        if t.as_str() == CotType::PING {
            Self::Ping
        } else if t.as_str() == CotType::PONG {
            Self::Pong
        } else if t.is_within("t-x-takp") {
            Self::ProtocolNegotiation
        } else {
            Self::Other
        }
    }
}
