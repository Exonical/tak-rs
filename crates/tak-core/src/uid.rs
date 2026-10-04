//! TAK object identifiers.

use std::fmt;
use std::str::FromStr;

use crate::error::{ValidationError, has_control_chars};

/// Maximum accepted UID length in bytes. Real-world UIDs are short
/// (`ANDROID-<hex>`, `S-1-5-21-...`, `GeoChat.<uid>.<room>.<msgid>`); the cap
/// exists only to bound memory on hostile input.
pub const MAX_UID_LEN: usize = 1024;

/// Unique identifier of a TAK entity (object, contact, chat room, message).
///
/// UIDs are opaque strings on the wire. TAK-RS only requires that they are
/// non-empty, contain no control characters and fit in [`MAX_UID_LEN`].
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub struct TakUid(String);

impl TakUid {
    /// Validate and wrap a UID string.
    pub fn new(uid: impl Into<String>) -> Result<Self, ValidationError> {
        let uid = uid.into();
        if uid.is_empty() {
            return Err(ValidationError::EmptyUid);
        }
        if uid.len() > MAX_UID_LEN || has_control_chars(&uid) || uid.trim() != uid {
            return Err(ValidationError::Uid(uid));
        }
        Ok(Self(uid))
    }

    /// The UID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the UID, returning the underlying string.
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Debug for TakUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TakUid({:?})", self.0)
    }
}

impl fmt::Display for TakUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for TakUid {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for TakUid {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TakUid> for String {
    fn from(uid: TakUid) -> Self {
        uid.0
    }
}

impl AsRef<str> for TakUid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_typical_uids() {
        for uid in [
            "ANDROID-1234abcd",
            "S-1-5-21-1",
            "GeoChat.a.b.c",
            "uid with space",
        ] {
            assert!(TakUid::new(uid).is_ok(), "{uid}");
        }
    }

    #[test]
    fn rejects_bad_uids() {
        assert_eq!(TakUid::new(""), Err(ValidationError::EmptyUid));
        assert!(TakUid::new("a\nb").is_err());
        assert!(TakUid::new(" leading").is_err());
        assert!(TakUid::new("x".repeat(MAX_UID_LEN + 1)).is_err());
    }
}
