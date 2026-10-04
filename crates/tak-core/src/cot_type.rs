//! The Cursor-on-Target type hierarchy (`a-f-G-U-C`, `b-t-f`, `t-x-d-d`, ...).
//!
//! A CoT type is a dash-separated path through a taxonomy. For *atoms*
//! (`a-...`) the second segment carries the affiliation and the third the
//! battle dimension, which is where TAK derives symbology colour from.

use std::fmt;
use std::str::FromStr;

use crate::error::ValidationError;

/// Maximum accepted CoT type length in bytes.
pub const MAX_COT_TYPE_LEN: usize = 256;

/// A validated CoT type string.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct CotType(String);

impl CotType {
    /// Generic friendly ground unit (default self marker type).
    pub const FRIENDLY_GROUND_UNIT: &'static str = "a-f-G-U-C";
    /// Generic unknown ground point.
    pub const UNKNOWN_GROUND: &'static str = "a-u-G";
    /// GeoChat message.
    pub const GEOCHAT: &'static str = "b-t-f";
    /// Delete / "force delete" tasking.
    pub const DELETE: &'static str = "t-x-d-d";
    /// Ping to TAK Server.
    pub const PING: &'static str = "t-x-c-t";
    /// Pong from TAK Server.
    pub const PONG: &'static str = "t-x-c-t-r";
    /// TAK Server takv control message (`takProtocol` negotiation).
    pub const TAK_CONTROL: &'static str = "t-x-takp-v";
    /// TAK Server protocol version acknowledgement.
    pub const TAK_CONTROL_RESPONSE: &'static str = "t-x-takp-r";
    /// Route (ATAK user drawing).
    pub const ROUTE: &'static str = "b-m-r";
    /// Generic point marker.
    pub const MARKER_POINT: &'static str = "b-m-p-s-m";
    /// Circle drawing.
    pub const SHAPE_CIRCLE: &'static str = "u-d-c-c";
    /// Freeform polygon drawing.
    pub const SHAPE_POLYGON: &'static str = "u-d-f";
    /// Rectangle drawing.
    pub const SHAPE_RECTANGLE: &'static str = "u-d-r";

    /// Validate a CoT type.
    ///
    /// Grammar: one or more non-empty segments of printable ASCII (no
    /// whitespace) separated by single dashes, e.g. `a-f-G-U-C`.
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_COT_TYPE_LEN {
            return Err(ValidationError::CotType(value));
        }
        let valid = value
            .split('-')
            .all(|seg| !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_graphic() && b != b'-'));
        if valid {
            Ok(Self(value))
        } else {
            Err(ValidationError::CotType(value))
        }
    }

    /// The type as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Iterator over the dash-separated segments.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('-')
    }

    /// Root of the taxonomy (first segment).
    pub fn root(&self) -> CotRoot {
        CotRoot::from_segment(self.segments().next().unwrap_or_default())
    }

    /// Whether this is an atom (`a-...`), i.e. a physical thing with a position.
    pub fn is_atom(&self) -> bool {
        self.root() == CotRoot::Atom
    }

    /// Affiliation for atoms, `None` for other roots or malformed atoms.
    pub fn affiliation(&self) -> Option<Affiliation> {
        if !self.is_atom() {
            return None;
        }
        self.segments().nth(1).and_then(Affiliation::from_segment)
    }

    /// Battle dimension for atoms, `None` for other roots or missing segment.
    pub fn dimension(&self) -> Option<BattleDimension> {
        if !self.is_atom() {
            return None;
        }
        self.segments()
            .nth(2)
            .and_then(BattleDimension::from_segment)
    }

    /// Whether `self` is `prefix` or a descendant of `prefix` in the taxonomy.
    ///
    /// `a-f-G-U-C` starts with `a-f-G` but not with `a-f-GX`.
    pub fn is_within(&self, prefix: &str) -> bool {
        self.0 == prefix
            || (self.0.starts_with(prefix) && self.0.as_bytes().get(prefix.len()) == Some(&b'-'))
    }
}

impl fmt::Debug for CotType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CotType({:?})", self.0)
    }
}

impl fmt::Display for CotType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for CotType {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for CotType {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<CotType> for String {
    fn from(t: CotType) -> Self {
        t.0
    }
}

/// First segment of a CoT type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CotRoot {
    /// `a` — atoms: things that exist in the world and have a position.
    Atom,
    /// `b` — bits: data products (chat, images, routes, drawings...).
    Bits,
    /// `c` — capabilities.
    Capability,
    /// `r` — replies.
    Reply,
    /// `t` — tasking (including TAK control messages).
    Tasking,
    /// `y` — reservations.
    Reservation,
    /// `u` — user drawings (TAK extension).
    UserDrawing,
    /// Anything else.
    Other,
}

impl CotRoot {
    fn from_segment(seg: &str) -> Self {
        match seg {
            "a" => Self::Atom,
            "b" => Self::Bits,
            "c" => Self::Capability,
            "r" => Self::Reply,
            "t" => Self::Tasking,
            "y" => Self::Reservation,
            "u" => Self::UserDrawing,
            _ => Self::Other,
        }
    }
}

/// Standard identity / affiliation of an atom (MIL-STD-2525 inspired).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Affiliation {
    /// `p`
    Pending,
    /// `u`
    Unknown,
    /// `a`
    AssumedFriend,
    /// `f`
    Friend,
    /// `n`
    Neutral,
    /// `s`
    Suspect,
    /// `h`
    Hostile,
    /// `j`
    Joker,
    /// `k`
    Faker,
    /// `o`
    None,
    /// `x`
    Other,
}

impl Affiliation {
    /// Parse the single-character CoT segment.
    pub fn from_segment(seg: &str) -> Option<Self> {
        Some(match seg {
            "p" => Self::Pending,
            "u" => Self::Unknown,
            "a" => Self::AssumedFriend,
            "f" => Self::Friend,
            "n" => Self::Neutral,
            "s" => Self::Suspect,
            "h" => Self::Hostile,
            "j" => Self::Joker,
            "k" => Self::Faker,
            "o" => Self::None,
            "x" => Self::Other,
            _ => return Option::None,
        })
    }

    /// The CoT segment for this affiliation.
    pub fn segment(self) -> &'static str {
        match self {
            Self::Pending => "p",
            Self::Unknown => "u",
            Self::AssumedFriend => "a",
            Self::Friend => "f",
            Self::Neutral => "n",
            Self::Suspect => "s",
            Self::Hostile => "h",
            Self::Joker => "j",
            Self::Faker => "k",
            Self::None => "o",
            Self::Other => "x",
        }
    }

    /// Whether this affiliation is rendered as friendly (blue) in TAK symbology.
    pub fn is_friendly(self) -> bool {
        matches!(self, Self::Friend | Self::AssumedFriend)
    }

    /// Whether this affiliation is rendered as hostile (red) in TAK symbology.
    pub fn is_hostile(self) -> bool {
        matches!(
            self,
            Self::Hostile | Self::Suspect | Self::Joker | Self::Faker
        )
    }
}

impl fmt::Display for Affiliation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pending => "Pending",
            Self::Unknown => "Unknown",
            Self::AssumedFriend => "Assumed Friend",
            Self::Friend => "Friendly",
            Self::Neutral => "Neutral",
            Self::Suspect => "Suspect",
            Self::Hostile => "Hostile",
            Self::Joker => "Joker",
            Self::Faker => "Faker",
            Self::None => "None",
            Self::Other => "Other",
        })
    }
}

/// Battle dimension of an atom (third CoT segment).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum BattleDimension {
    /// `P`
    Space,
    /// `A`
    Air,
    /// `G`
    Ground,
    /// `S`
    SeaSurface,
    /// `U`
    SeaSubsurface,
    /// `F`
    SpecialOperations,
    /// `X`
    Other,
}

impl BattleDimension {
    /// Parse the single-character CoT segment.
    pub fn from_segment(seg: &str) -> Option<Self> {
        Some(match seg {
            "P" => Self::Space,
            "A" => Self::Air,
            "G" => Self::Ground,
            "S" => Self::SeaSurface,
            "U" => Self::SeaSubsurface,
            "F" => Self::SpecialOperations,
            "X" => Self::Other,
            _ => return None,
        })
    }

    /// The CoT segment for this dimension.
    pub fn segment(self) -> &'static str {
        match self {
            Self::Space => "P",
            Self::Air => "A",
            Self::Ground => "G",
            Self::SeaSurface => "S",
            Self::SeaSubsurface => "U",
            Self::SpecialOperations => "F",
            Self::Other => "X",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_atoms() {
        let t = CotType::new("a-f-G-U-C").unwrap();
        assert_eq!(t.root(), CotRoot::Atom);
        assert_eq!(t.affiliation(), Some(Affiliation::Friend));
        assert_eq!(t.dimension(), Some(BattleDimension::Ground));
        assert!(t.is_within("a-f-G"));
        assert!(t.is_within("a-f-G-U-C"));
        assert!(!t.is_within("a-f-G-U-CX"));
        assert!(!t.is_within("a-f-GX"));
    }

    #[test]
    fn non_atoms_have_no_affiliation() {
        let t = CotType::new("b-t-f").unwrap();
        assert_eq!(t.root(), CotRoot::Bits);
        assert_eq!(t.affiliation(), None);
        assert_eq!(CotType::new("t-x-d-d").unwrap().root(), CotRoot::Tasking);
    }

    #[test]
    fn rejects_malformed() {
        for bad in ["", "a--f", "-a", "a-", "a f", "a-f-\u{e9}", "a\n"] {
            assert!(CotType::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn affiliation_roundtrip() {
        for a in [
            Affiliation::Pending,
            Affiliation::Unknown,
            Affiliation::AssumedFriend,
            Affiliation::Friend,
            Affiliation::Neutral,
            Affiliation::Suspect,
            Affiliation::Hostile,
            Affiliation::Joker,
            Affiliation::Faker,
            Affiliation::None,
            Affiliation::Other,
        ] {
            assert_eq!(Affiliation::from_segment(a.segment()), Some(a));
        }
    }
}
