//! The generic TAK object: anything with a position and a CoT type.

use crate::callsign::Callsign;
use crate::color::Argb;
use crate::cot_type::{Affiliation, CotType};
use crate::extension::Extensions;
use crate::geo::{Altitude, Precision};
use crate::geometry::Geometry;
use crate::how::How;
use crate::source::ObjectSource;
use crate::time::{Duration, Timestamp, Validity};
use crate::uid::TakUid;

/// A map object: marker, shape, route, sensor, unit, ...
///
/// Contacts (other TAK users) are a specialisation modelled separately as
/// [`crate::Contact`] because they carry team/role/device data and are
/// tracked over time; everything else is a `TakObject`.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TakObject {
    /// Stable identifier.
    pub uid: TakUid,
    /// CoT type (taxonomy path).
    pub cot_type: CotType,
    /// Affiliation, derived from `cot_type` for atoms; `Unknown` otherwise
    /// unless the producer knows better.
    pub affiliation: Affiliation,
    /// Position / shape.
    pub geometry: Geometry,
    /// Height above ellipsoid of the anchor point, if known.
    pub altitude: Option<Altitude>,
    /// Horizontal and vertical error estimates.
    pub precision: Precision,
    /// Provenance of the position.
    pub how: How,
    /// Display metadata and preserved extensions.
    pub metadata: ObjectMetadata,
    /// Where this object came from.
    pub source: ObjectSource,
    /// Validity window and receipt time.
    pub timestamps: ObjectTimestamps,
}

impl TakObject {
    /// Create a point object with sensible defaults (`how = h-e`, white colour,
    /// 1 hour validity from `now`).
    pub fn point(
        uid: TakUid,
        cot_type: CotType,
        position: crate::geo::GeoPoint,
        now: Timestamp,
    ) -> Self {
        let affiliation = cot_type.affiliation().unwrap_or(Affiliation::Unknown);
        Self {
            uid,
            cot_type,
            affiliation,
            geometry: Geometry::Point(position),
            altitude: None,
            precision: Precision::UNKNOWN,
            how: How::human_entered(),
            metadata: ObjectMetadata::default(),
            source: ObjectSource::LOCAL,
            timestamps: ObjectTimestamps::from_validity(Validity::from_now(
                now,
                Duration::hours(1),
            )),
        }
    }

    /// Whether the object has passed its stale time at `now`.
    pub fn is_stale_at(&self, now: Timestamp) -> bool {
        self.timestamps.validity.is_stale_at(now)
    }

    /// Callsign if present in metadata.
    pub fn callsign(&self) -> Option<&Callsign> {
        self.metadata.callsign.as_ref()
    }
}

/// Human-facing metadata plus extension payloads.
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectMetadata {
    /// Display name.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub callsign: Option<Callsign>,
    /// Free-text remarks.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub remarks: Option<String>,
    /// Primary colour (marker tint / stroke).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub color: Option<Argb>,
    /// Fill colour for shapes.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub fill_color: Option<Argb>,
    /// Stroke weight for shapes, in logical pixels.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub stroke_weight: Option<f64>,
    /// Icon set path (TAK `usericon iconsetpath`).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub icon: Option<String>,
    /// Relationship to another object (e.g. producer, parent shape).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub links: Vec<Link>,
    /// Whether the originator asked for the object to be archived/persisted.
    #[cfg_attr(feature = "serde", serde(default))]
    pub archive: bool,
    /// Detail elements TAK-RS does not model natively, preserved verbatim.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Extensions::is_empty")
    )]
    pub extensions: Extensions,
}

/// A typed relationship to another object (CoT `<link>`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Link {
    /// UID of the related object.
    pub uid: TakUid,
    /// Relationship, e.g. `p-p` (parent-producer).
    pub relation: String,
    /// CoT type of the related object, if known.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub cot_type: Option<CotType>,
    /// Callsign of the related object, if known.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub callsign: Option<Callsign>,
}

/// Validity window plus local receipt time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ObjectTimestamps {
    /// `time`/`start`/`stale` from the originator.
    pub validity: Validity,
    /// When this device received the object (`None` for local objects).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub received: Option<Timestamp>,
}

impl ObjectTimestamps {
    /// Wrap a validity window with no receipt time.
    pub fn from_validity(validity: Validity) -> Self {
        Self {
            validity,
            received: None,
        }
    }

    /// Builder: record the receipt time.
    pub fn received_at(mut self, at: Timestamp) -> Self {
        self.received = Some(at);
        self
    }
}
