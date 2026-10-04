//! Other TAK users on the network.

use crate::callsign::Callsign;
use crate::cot_type::{Affiliation, CotType};
use crate::extension::Extensions;
use crate::geo::{Altitude, GeoPoint, Heading, Precision, Speed};
use crate::how::How;
use crate::object::ObjectTimestamps;
use crate::source::ObjectSource;
use crate::team::{Role, Team};
use crate::time::Timestamp;
use crate::track::{TrackPoint, TrackSource};
use crate::uid::TakUid;

/// A TAK user (ATAK/WinTAK/iTAK/TAK-RS instance) known from position reports.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Contact {
    /// Device/user identifier.
    pub uid: TakUid,
    /// Display name.
    pub callsign: Callsign,
    /// CoT type of the self marker (usually `a-f-G-U-C`).
    pub cot_type: CotType,
    /// Affiliation derived from `cot_type`.
    pub affiliation: Affiliation,
    /// Team colour.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub team: Option<Team>,
    /// Team role.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub role: Option<Role>,
    /// Client software/hardware.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub device: Option<DeviceInfo>,
    /// Last reported position.
    pub position: GeoPoint,
    /// Last reported altitude.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub altitude: Option<Altitude>,
    /// Position error estimates.
    #[cfg_attr(feature = "serde", serde(default))]
    pub precision: Precision,
    /// Ground speed.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub speed: Option<Speed>,
    /// Course over ground.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub heading: Option<Heading>,
    /// Provenance of the position.
    pub how: How,
    /// Battery percentage, if reported.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub battery_percent: Option<u8>,
    /// Direct-messaging endpoint advertised by the contact (e.g. `*:-1:stcp`).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub endpoint: Option<String>,
    /// Phone number, if advertised.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub phone: Option<String>,
    /// Where the report came from.
    pub source: ObjectSource,
    /// Validity window of the last report and when we received it.
    pub timestamps: ObjectTimestamps,
    /// Detail elements not modelled natively.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Extensions::is_empty")
    )]
    pub extensions: Extensions,
}

impl Contact {
    /// Whether the last report has gone stale at `now`.
    pub fn is_stale_at(&self, now: Timestamp) -> bool {
        self.timestamps.validity.is_stale_at(now)
    }

    /// Age of the last report at `now` (zero if the report is from the future).
    pub fn age_at(&self, now: Timestamp) -> crate::time::Duration {
        now.saturating_since(self.timestamps.validity.time)
    }

    /// Convert the current report into a track sample.
    pub fn to_track_point(&self) -> TrackPoint {
        TrackPoint {
            timestamp: self.timestamps.validity.time,
            position: self.position,
            altitude: self.altitude,
            speed: self.speed,
            heading: self.heading,
            source: TrackSource::from_how(&self.how),
        }
    }
}

/// Client software / hardware description (CoT `<takv>`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DeviceInfo {
    /// Hardware model, e.g. `Pixel 7`.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub device: Option<String>,
    /// Client platform, e.g. `ATAK-CIV`, `WinTAK`, `TAK-RS`.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub platform: Option<String>,
    /// Operating system identifier.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub os: Option<String>,
    /// Client version string.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub version: Option<String>,
}

impl DeviceInfo {
    /// Whether no field is populated.
    pub fn is_empty(&self) -> bool {
        self.device.is_none()
            && self.platform.is_none()
            && self.os.is_none()
            && self.version.is_none()
    }
}
