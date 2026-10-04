//! Human-readable contact and object names.

use std::fmt;
use std::str::FromStr;

use crate::error::{ValidationError, has_control_chars};

/// Maximum accepted callsign length in bytes.
pub const MAX_CALLSIGN_LEN: usize = 256;

/// A callsign (display name) for a contact or object.
///
/// Leading and trailing whitespace is trimmed. Callsigns must be non-empty and
/// contain no control characters.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct Callsign(String);

impl Callsign {
    /// Validate and wrap a callsign.
    pub fn new(callsign: impl AsRef<str>) -> Result<Self, ValidationError> {
        let trimmed = callsign.as_ref().trim();
        if trimmed.is_empty() {
            return Err(ValidationError::EmptyCallsign);
        }
        if trimmed.len() > MAX_CALLSIGN_LEN || has_control_chars(trimmed) {
            return Err(ValidationError::Callsign(trimmed.to_owned()));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The callsign as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Callsign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Callsign({:?})", self.0)
    }
}

impl fmt::Display for Callsign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Callsign {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for Callsign {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Callsign> for String {
    fn from(c: Callsign) -> Self {
        c.0
    }
}

impl AsRef<str> for Callsign {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_validates() {
        assert_eq!(Callsign::new("  ALPHA-01 ").unwrap().as_str(), "ALPHA-01");
        assert_eq!(Callsign::new("   "), Err(ValidationError::EmptyCallsign));
        assert!(Callsign::new("bad\u{7}").is_err());
    }
}
