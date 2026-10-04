//! Track history samples.
//!
//! Tracking is never "latest location only": every position report becomes
//! a [`TrackPoint`] so history, playback and time-range queries are possible.

use crate::geo::{Altitude, GeoPoint, Heading, Speed};
use crate::how::{How, HowCategory};
use crate::time::Timestamp;

/// One sample of an entity's movement.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackPoint {
    /// When the sample was taken (originator time).
    pub timestamp: Timestamp,
    /// Position.
    pub position: GeoPoint,
    /// Altitude, if known.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub altitude: Option<Altitude>,
    /// Ground speed, if known.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub speed: Option<Speed>,
    /// Course, if known.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub heading: Option<Heading>,
    /// How the sample was produced.
    pub source: TrackSource,
}

/// Provenance class of a track sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TrackSource {
    /// Satellite positioning on the device.
    Gps,
    /// Fused / filtered from several sensors.
    Fused,
    /// Dead-reckoned or predicted.
    Predicted,
    /// Relayed by another node.
    Relayed,
    /// Simulated.
    Simulated,
    /// Entered or dragged by a human.
    Manual,
    /// Could not be classified.
    Unknown,
}

impl TrackSource {
    /// Classify from the CoT `how` attribute.
    pub fn from_how(how: &How) -> Self {
        match how.as_str() {
            How::MACHINE_GPS => Self::Gps,
            How::MACHINE_FUSED => Self::Fused,
            How::MACHINE_PREDICTED => Self::Predicted,
            How::MACHINE_RELAYED => Self::Relayed,
            How::MACHINE_SIMULATED => Self::Simulated,
            _ => match how.category() {
                HowCategory::Human => Self::Manual,
                HowCategory::Machine | HowCategory::Unknown => Self::Unknown,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_how() {
        assert_eq!(TrackSource::from_how(&How::machine_gps()), TrackSource::Gps);
        assert_eq!(
            TrackSource::from_how(&How::human_gigo()),
            TrackSource::Manual
        );
        assert_eq!(
            TrackSource::from_how(&How::new("m-z").unwrap()),
            TrackSource::Unknown
        );
    }
}
