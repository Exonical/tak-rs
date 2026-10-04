//! Chat model (TAK GeoChat compatible).

use crate::callsign::Callsign;
use crate::time::Timestamp;
use crate::uid::TakUid;

/// What kind of conversation a message belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ConversationKind {
    /// One-to-one with the contact identified by the conversation id.
    Direct,
    /// The "All Chat Rooms" broadcast channel.
    Broadcast,
    /// A named group / team / mission channel.
    Group,
}

/// A conversation (room) identifier plus display name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Conversation {
    /// Room/contact identifier (`All Chat Rooms`, a contact UID, a team name...).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Kind of conversation.
    pub kind: ConversationKind,
}

impl Conversation {
    /// Identifier TAK clients use for the broadcast room.
    pub const ALL_CHAT_ROOMS: &'static str = "All Chat Rooms";

    /// The broadcast conversation.
    pub fn broadcast() -> Self {
        Self {
            id: Self::ALL_CHAT_ROOMS.to_owned(),
            name: Self::ALL_CHAT_ROOMS.to_owned(),
            kind: ConversationKind::Broadcast,
        }
    }

    /// A direct conversation with `contact`.
    pub fn direct(contact: &TakUid, callsign: &Callsign) -> Self {
        Self {
            id: contact.as_str().to_owned(),
            name: callsign.as_str().to_owned(),
            kind: ConversationKind::Direct,
        }
    }
}

/// A chat message.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChatMessage {
    /// Unique message identifier (used for de-duplication and read/delivery state).
    pub message_id: TakUid,
    /// Sender device UID.
    pub sender: TakUid,
    /// Sender callsign at time of sending.
    pub sender_callsign: Callsign,
    /// Conversation this message belongs to.
    pub conversation: Conversation,
    /// Explicit recipients for direct/group messages.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub recipients: Vec<TakUid>,
    /// Message body.
    pub text: String,
    /// When the sender created the message.
    pub sent_at: Timestamp,
}
