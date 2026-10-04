//! Wire-faithful representation of a CoT `<event>`.
//!
//! [`CotEvent`] mirrors the CoT 2.0 schema closely: it keeps the raw point
//! values (including the `9999999.0` "unknown" sentinel), optional
//! classification attributes, unknown `<event>` attributes, and the full
//! `<detail>` subtree. Conversion to the domain model lives in
//! [`crate::adapter`].

use tak_core::{CotType, Extensions, How, TakUid, Timestamp, Validity};

use crate::error::CotError;

/// The only CoT schema version TAK-RS emits.
pub const COT_VERSION: &str = "2.0";

/// Sentinel used by TAK clients for "unknown" altitude / error values.
pub const UNKNOWN_SENTINEL: f64 = 9_999_999.0;

/// The `<point>` element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CotPoint {
    /// Latitude, decimal degrees WGS-84.
    pub lat: f64,
    /// Longitude, decimal degrees WGS-84.
    pub lon: f64,
    /// Height above the WGS-84 ellipsoid in metres, or [`UNKNOWN_SENTINEL`].
    pub hae: f64,
    /// Circular (horizontal) error in metres, or [`UNKNOWN_SENTINEL`].
    pub ce: f64,
    /// Linear (vertical) error in metres, or [`UNKNOWN_SENTINEL`].
    pub le: f64,
}

impl CotPoint {
    /// A point with unknown altitude and precision.
    pub fn new(lat: f64, lon: f64) -> Result<Self, CotError> {
        let p = Self {
            lat,
            lon,
            hae: UNKNOWN_SENTINEL,
            ce: UNKNOWN_SENTINEL,
            le: UNKNOWN_SENTINEL,
        };
        p.validate()?;
        Ok(p)
    }

    /// The conventional "no position" point used by chat and control events.
    pub const NULL_ISLAND: Self = Self {
        lat: 0.0,
        lon: 0.0,
        hae: UNKNOWN_SENTINEL,
        ce: UNKNOWN_SENTINEL,
        le: UNKNOWN_SENTINEL,
    };

    /// Check ranges: lat ∈ [-90, 90], lon ∈ [-180, 180], all values finite.
    pub fn validate(&self) -> Result<(), CotError> {
        let check = |name: &str, v: f64, lo: f64, hi: f64| {
            if !v.is_finite() || v < lo || v > hi {
                return Err(CotError::invalid_attr(
                    "point",
                    name,
                    &v.to_string(),
                    format!("must be finite and within [{lo}, {hi}]"),
                ));
            }
            Ok(())
        };
        check("lat", self.lat, -90.0, 90.0)?;
        check("lon", self.lon, -180.0, 180.0)?;
        for (name, v) in [("hae", self.hae), ("ce", self.ce), ("le", self.le)] {
            if !v.is_finite() {
                return Err(CotError::invalid_attr(
                    "point",
                    name,
                    &v.to_string(),
                    "must be finite",
                ));
            }
        }
        Ok(())
    }

    /// Altitude in metres HAE, or `None` when the sentinel is used.
    pub fn hae_metres(&self) -> Option<f64> {
        Self::known(self.hae)
    }

    /// Circular error in metres, or `None` when unknown.
    pub fn ce_metres(&self) -> Option<f64> {
        Self::known(self.ce).filter(|v| *v >= 0.0)
    }

    /// Linear error in metres, or `None` when unknown.
    pub fn le_metres(&self) -> Option<f64> {
        Self::known(self.le).filter(|v| *v >= 0.0)
    }

    fn known(v: f64) -> Option<f64> {
        (v.is_finite() && v < UNKNOWN_SENTINEL).then_some(v)
    }
}

/// A complete CoT `<event>`.
#[derive(Clone, Debug, PartialEq)]
pub struct CotEvent {
    /// Schema version (`2.0`).
    pub version: String,
    /// Globally unique identifier of the thing this event describes.
    pub uid: TakUid,
    /// CoT type hierarchy string.
    pub cot_type: CotType,
    /// How the information was obtained. Required by the schema, but some
    /// emitters omit it, so it is optional on input.
    pub how: Option<How>,
    /// When the event was generated.
    pub time: Timestamp,
    /// Start of validity.
    pub start: Timestamp,
    /// End of validity.
    pub stale: Timestamp,
    /// `access` attribute (TAK: `Undefined`, `Unclassified`, ...).
    pub access: Option<String>,
    /// `opex` attribute (operation / exercise / simulation marker).
    pub opex: Option<String>,
    /// `qos` attribute.
    pub qos: Option<String>,
    /// `caveat` attribute.
    pub caveat: Option<String>,
    /// `releaseableTo` attribute.
    pub releaseable_to: Option<String>,
    /// Any `<event>` attributes not covered above, in document order.
    pub extra_attributes: Vec<(String, String)>,
    /// The `<point>` child.
    pub point: CotPoint,
    /// Children of `<detail>`, in document order.
    pub detail: Extensions,
}

impl CotEvent {
    /// A minimal valid event with `start == time` and no `how`.
    pub fn new(
        uid: TakUid,
        cot_type: CotType,
        point: CotPoint,
        time: Timestamp,
        stale: Timestamp,
    ) -> Self {
        Self {
            version: COT_VERSION.to_owned(),
            uid,
            cot_type,
            how: None,
            time,
            start: time,
            stale,
            access: None,
            opex: None,
            qos: None,
            caveat: None,
            releaseable_to: None,
            extra_attributes: Vec::new(),
            point,
            detail: Extensions::new(),
        }
    }

    /// Set the `how` attribute.
    pub fn with_how(mut self, how: How) -> Self {
        self.how = Some(how);
        self
    }

    /// Replace the detail subtree.
    pub fn with_detail(mut self, detail: Extensions) -> Self {
        self.detail = detail;
        self
    }

    /// The validity window, rejecting `start > stale`.
    pub fn validity(&self) -> Result<Validity, CotError> {
        Ok(Validity::new(self.time, self.start, self.stale)?)
    }

    /// Whether the event should be considered stale at `now`.
    pub fn is_stale_at(&self, now: Timestamp) -> bool {
        now > self.stale
    }
}
