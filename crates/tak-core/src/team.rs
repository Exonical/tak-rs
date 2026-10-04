//! TAK team colours and team roles as carried in the `__group` detail.

use std::fmt;
use std::str::FromStr;

use crate::error::ValidationError;

const MAX_LEN: usize = 64;

fn validate_label(field: &'static str, s: &str) -> Result<(), ValidationError> {
    if s.is_empty() {
        return Err(ValidationError::Text {
            field,
            reason: "must not be empty",
        });
    }
    if s.len() > MAX_LEN {
        return Err(ValidationError::Text {
            field,
            reason: "too long",
        });
    }
    if s.chars().any(char::is_control) {
        return Err(ValidationError::Text {
            field,
            reason: "contains control characters",
        });
    }
    Ok(())
}

/// Team colour. The named variants are the colours TAK clients ship with;
/// `Custom` preserves anything else seen on the wire.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub enum Team {
    /// White
    White,
    /// Yellow
    Yellow,
    /// Orange
    Orange,
    /// Magenta
    Magenta,
    /// Red
    Red,
    /// Maroon
    Maroon,
    /// Purple
    Purple,
    /// Dark Blue
    DarkBlue,
    /// Blue
    Blue,
    /// Cyan
    Cyan,
    /// Teal
    Teal,
    /// Green
    Green,
    /// Dark Green
    DarkGreen,
    /// Brown
    Brown,
    /// Any other team label.
    Custom(String),
}

impl Team {
    /// All built-in teams, in the order TAK clients present them.
    pub const BUILTIN: [Team; 14] = [
        Team::White,
        Team::Yellow,
        Team::Orange,
        Team::Magenta,
        Team::Red,
        Team::Maroon,
        Team::Purple,
        Team::DarkBlue,
        Team::Blue,
        Team::Cyan,
        Team::Teal,
        Team::Green,
        Team::DarkGreen,
        Team::Brown,
    ];

    /// Parse the TAK wire label (`"Dark Blue"`, `"Cyan"`, ...).
    pub fn parse(label: &str) -> Result<Self, ValidationError> {
        let label = label.trim();
        validate_label("team", label)?;
        Ok(Self::BUILTIN
            .iter()
            .find(|t| t.as_str() == label)
            .cloned()
            .unwrap_or_else(|| Self::Custom(label.to_owned())))
    }

    /// The TAK wire label.
    pub fn as_str(&self) -> &str {
        match self {
            Self::White => "White",
            Self::Yellow => "Yellow",
            Self::Orange => "Orange",
            Self::Magenta => "Magenta",
            Self::Red => "Red",
            Self::Maroon => "Maroon",
            Self::Purple => "Purple",
            Self::DarkBlue => "Dark Blue",
            Self::Blue => "Blue",
            Self::Cyan => "Cyan",
            Self::Teal => "Teal",
            Self::Green => "Green",
            Self::DarkGreen => "Dark Green",
            Self::Brown => "Brown",
            Self::Custom(s) => s,
        }
    }
}

impl fmt::Display for Team {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Team {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for Team {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Team> for String {
    fn from(t: Team) -> Self {
        t.as_str().to_owned()
    }
}

/// Role within a team.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
pub enum Role {
    /// Team Member
    TeamMember,
    /// Team Lead
    TeamLead,
    /// HQ
    Hq,
    /// Sniper
    Sniper,
    /// Medic
    Medic,
    /// Forward Observer
    ForwardObserver,
    /// RTO
    Rto,
    /// K9
    K9,
    /// Any other role label.
    Custom(String),
}

impl Role {
    /// All built-in roles.
    pub const BUILTIN: [Role; 8] = [
        Role::TeamMember,
        Role::TeamLead,
        Role::Hq,
        Role::Sniper,
        Role::Medic,
        Role::ForwardObserver,
        Role::Rto,
        Role::K9,
    ];

    /// Parse the TAK wire label.
    pub fn parse(label: &str) -> Result<Self, ValidationError> {
        let label = label.trim();
        validate_label("role", label)?;
        Ok(Self::BUILTIN
            .iter()
            .find(|r| r.as_str() == label)
            .cloned()
            .unwrap_or_else(|| Self::Custom(label.to_owned())))
    }

    /// The TAK wire label.
    pub fn as_str(&self) -> &str {
        match self {
            Self::TeamMember => "Team Member",
            Self::TeamLead => "Team Lead",
            Self::Hq => "HQ",
            Self::Sniper => "Sniper",
            Self::Medic => "Medic",
            Self::ForwardObserver => "Forward Observer",
            Self::Rto => "RTO",
            Self::K9 => "K9",
            Self::Custom(s) => s,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for Role {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Role> for String {
    fn from(r: Role) -> Self {
        r.as_str().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_builtin() {
        for t in Team::BUILTIN {
            assert_eq!(Team::parse(t.as_str()).unwrap(), t);
        }
        for r in Role::BUILTIN {
            assert_eq!(Role::parse(r.as_str()).unwrap(), r);
        }
    }

    #[test]
    fn custom_preserved() {
        assert_eq!(
            Team::parse("Chartreuse").unwrap(),
            Team::Custom("Chartreuse".into())
        );
        assert_eq!(
            Role::parse(" Pilot ").unwrap(),
            Role::Custom("Pilot".into())
        );
        assert!(Team::parse("").is_err());
    }
}
