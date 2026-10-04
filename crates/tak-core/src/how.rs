//! The CoT `how` attribute: provenance of the position.

use std::fmt;
use std::str::FromStr;

use crate::error::ValidationError;

/// Maximum accepted `how` length in bytes.
pub const MAX_HOW_LEN: usize = 64;

/// How a position was derived (`m-g` = machine GPS, `h-e` = human entered, ...).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct How(String);

impl How {
    /// Machine generated from GPS.
    pub const MACHINE_GPS: &'static str = "m-g";
    /// Machine fused from multiple sources.
    pub const MACHINE_FUSED: &'static str = "m-f";
    /// Machine predicted / dead reckoned.
    pub const MACHINE_PREDICTED: &'static str = "m-p";
    /// Machine relayed.
    pub const MACHINE_RELAYED: &'static str = "m-r";
    /// Machine simulated.
    pub const MACHINE_SIMULATED: &'static str = "m-s";
    /// Machine from imagery.
    pub const MACHINE_IMAGERY: &'static str = "m-i";
    /// Human entered.
    pub const HUMAN_ENTERED: &'static str = "h-e";
    /// Human, garbage-in-garbage-out (used by ATAK for user drawings).
    pub const HUMAN_GIGO: &'static str = "h-g-i-g-o";

    /// Validate a `how` value (same grammar as [`crate::CotType`]).
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_HOW_LEN {
            return Err(ValidationError::How(value));
        }
        let valid = value
            .split('-')
            .all(|seg| !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_graphic() && b != b'-'));
        if valid {
            Ok(Self(value))
        } else {
            Err(ValidationError::How(value))
        }
    }

    /// `m-g`.
    pub fn machine_gps() -> Self {
        Self(Self::MACHINE_GPS.to_owned())
    }

    /// `h-e`.
    pub fn human_entered() -> Self {
        Self(Self::HUMAN_ENTERED.to_owned())
    }

    /// `h-g-i-g-o`.
    pub fn human_gigo() -> Self {
        Self(Self::HUMAN_GIGO.to_owned())
    }

    /// The value as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Broad category derived from the first segment.
    pub fn category(&self) -> HowCategory {
        match self.0.split('-').next() {
            Some("h") => HowCategory::Human,
            Some("m") => HowCategory::Machine,
            _ => HowCategory::Unknown,
        }
    }
}

impl fmt::Debug for How {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "How({:?})", self.0)
    }
}

impl fmt::Display for How {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for How {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for How {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<How> for String {
    fn from(h: How) -> Self {
        h.0
    }
}

/// Whether a position came from a human or a machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum HowCategory {
    /// `h-*`
    Human,
    /// `m-*`
    Machine,
    /// Anything else.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(How::machine_gps().category(), HowCategory::Machine);
        assert_eq!(How::human_gigo().category(), HowCategory::Human);
        assert_eq!(How::new("x-y").unwrap().category(), HowCategory::Unknown);
        assert!(How::new("").is_err());
        assert!(How::new("m g").is_err());
    }
}
