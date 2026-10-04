//! Where an object or event came from.

use std::fmt;

/// Identifier of a configured transport / connection (e.g. a server profile
/// name or `"multicast"`). Opaque to the domain model.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct TransportId(String);

impl TransportId {
    /// Wrap a transport identifier.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for TransportId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TransportId({:?})", self.0)
    }
}

impl fmt::Display for TransportId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Origin of a piece of state.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "origin", rename_all = "snake_case"))]
pub enum Origin {
    /// Created on this device by the local user or a local device provider.
    Local,
    /// Received over a transport.
    Remote {
        /// Transport the data arrived on.
        transport: TransportId,
    },
    /// Loaded from local persistence (origin before restart unknown).
    Stored,
}

/// Provenance of an object: origin plus the wire format it arrived in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectSource {
    /// Where the object came from.
    pub origin: Origin,
    /// Wire encoding the object was last received in, if remote.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub encoding: Option<WireEncoding>,
}

impl ObjectSource {
    /// Local origin.
    pub const LOCAL: Self = Self {
        origin: Origin::Local,
        encoding: None,
    };

    /// Remote origin over `transport` using `encoding`.
    pub fn remote(transport: TransportId, encoding: WireEncoding) -> Self {
        Self {
            origin: Origin::Remote { transport },
            encoding: Some(encoding),
        }
    }
}

impl Default for ObjectSource {
    fn default() -> Self {
        Self::LOCAL
    }
}

/// Wire encodings TAK-RS understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum WireEncoding {
    /// Legacy Cursor-on-Target XML.
    CotXml,
    /// TAK Protocol protobuf.
    TakProtobuf,
}
