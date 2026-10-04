//! Geodetic primitives: coordinates, altitude, precision, speed, heading.
//!
//! All angles are degrees, all lengths are metres, all altitudes are height
//! above the WGS-84 ellipsoid (HAE) unless a type says otherwise.

use std::fmt;

use crate::error::ValidationError;

/// Mean Earth radius in metres (IUGG), used for great-circle calculations.
pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Geodetic latitude in decimal degrees, `[-90, 90]`.
#[derive(Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Latitude(f64);

impl Latitude {
    /// Validate a latitude in degrees.
    pub fn new(degrees: f64) -> Result<Self, ValidationError> {
        if degrees.is_finite() && (-90.0..=90.0).contains(&degrees) {
            Ok(Self(degrees))
        } else {
            Err(ValidationError::Latitude(degrees))
        }
    }

    /// Latitude in decimal degrees.
    pub fn degrees(self) -> f64 {
        self.0
    }

    /// Latitude in radians.
    pub fn radians(self) -> f64 {
        self.0.to_radians()
    }
}

/// Geodetic longitude in decimal degrees, `[-180, 180]`.
#[derive(Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Longitude(f64);

impl Longitude {
    /// Validate a longitude in degrees.
    pub fn new(degrees: f64) -> Result<Self, ValidationError> {
        if degrees.is_finite() && (-180.0..=180.0).contains(&degrees) {
            Ok(Self(degrees))
        } else {
            Err(ValidationError::Longitude(degrees))
        }
    }

    /// Longitude in decimal degrees.
    pub fn degrees(self) -> f64 {
        self.0
    }

    /// Longitude in radians.
    pub fn radians(self) -> f64 {
        self.0.to_radians()
    }
}

/// A WGS-84 position on the ellipsoid surface (no altitude).
#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GeoPoint {
    /// Latitude.
    pub lat: Latitude,
    /// Longitude.
    pub lon: Longitude,
}

impl GeoPoint {
    /// Null Island (0°, 0°).
    pub const ORIGIN: Self = Self {
        lat: Latitude(0.0),
        lon: Longitude(0.0),
    };

    /// Construct from already validated components.
    pub fn new(lat: Latitude, lon: Longitude) -> Self {
        Self { lat, lon }
    }

    /// Validate and construct from decimal degrees.
    pub fn from_degrees(lat: f64, lon: f64) -> Result<Self, ValidationError> {
        Ok(Self {
            lat: Latitude::new(lat)?,
            lon: Longitude::new(lon)?,
        })
    }

    /// Great-circle distance to `other` using the haversine formula.
    pub fn distance_to(self, other: GeoPoint) -> Distance {
        let (lat1, lat2) = (self.lat.radians(), other.lat.radians());
        let dlat = lat2 - lat1;
        let dlon = other.lon.radians() - self.lon.radians();
        let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        let c = 2.0 * a.sqrt().min(1.0).asin();
        Distance(EARTH_RADIUS_M * c)
    }

    /// Initial great-circle bearing from `self` towards `other`.
    pub fn bearing_to(self, other: GeoPoint) -> Heading {
        let (lat1, lat2) = (self.lat.radians(), other.lat.radians());
        let dlon = other.lon.radians() - self.lon.radians();
        let y = dlon.sin() * lat2.cos();
        let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
        Heading::from_degrees_wrapping(y.atan2(x).to_degrees())
    }
}

impl fmt::Debug for GeoPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GeoPoint({:.6}, {:.6})", self.lat.0, self.lon.0)
    }
}

impl fmt::Display for GeoPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}, {:.6}", self.lat.0, self.lon.0)
    }
}

/// Height above the WGS-84 ellipsoid in metres.
///
/// Unknown altitude is modelled as `Option<Altitude>`, never as a sentinel.
#[derive(Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Altitude(f64);

impl Altitude {
    /// Validate an altitude (must be finite).
    pub fn hae(metres: f64) -> Result<Self, ValidationError> {
        if metres.is_finite() {
            Ok(Self(metres))
        } else {
            Err(ValidationError::Altitude(metres))
        }
    }

    /// Height above ellipsoid in metres.
    pub fn metres(self) -> f64 {
        self.0
    }
}

/// Horizontal (CE) and vertical (LE) error estimates in metres.
///
/// `None` means the originator did not provide an estimate. CoT encodes this
/// with the sentinel `9999999.0`; the adapter translates to and from `None`.
#[derive(Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Precision {
    /// Circular error, metres.
    pub ce_m: Option<f64>,
    /// Linear (vertical) error, metres.
    pub le_m: Option<f64>,
}

impl Precision {
    /// Unknown precision in both axes.
    pub const UNKNOWN: Self = Self {
        ce_m: None,
        le_m: None,
    };

    /// Validate error estimates; `None` passes through.
    pub fn new(ce_m: Option<f64>, le_m: Option<f64>) -> Result<Self, ValidationError> {
        for v in [ce_m, le_m].into_iter().flatten() {
            if !v.is_finite() || v < 0.0 {
                return Err(ValidationError::Precision(v));
            }
        }
        Ok(Self { ce_m, le_m })
    }
}

impl fmt::Debug for Precision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Precision(ce={:?}, le={:?})", self.ce_m, self.le_m)
    }
}

/// Ground speed in metres per second (non-negative).
#[derive(Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Speed(f64);

impl Speed {
    /// Validate a speed in m/s.
    pub fn mps(value: f64) -> Result<Self, ValidationError> {
        if value.is_finite() && value >= 0.0 {
            Ok(Self(value))
        } else {
            Err(ValidationError::Speed(value))
        }
    }

    /// Speed in m/s.
    pub fn as_mps(self) -> f64 {
        self.0
    }

    /// Speed in km/h.
    pub fn as_kph(self) -> f64 {
        self.0 * 3.6
    }

    /// Speed in knots.
    pub fn as_knots(self) -> f64 {
        self.0 * 1.943_844_5
    }
}

/// Direction in degrees clockwise from true north, normalised to `[0, 360)`.
#[derive(Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Heading(f64);

impl Heading {
    /// Validate a heading. Any finite value is accepted and wrapped into `[0, 360)`.
    pub fn degrees(value: f64) -> Result<Self, ValidationError> {
        if value.is_finite() {
            Ok(Self::from_degrees_wrapping(value))
        } else {
            Err(ValidationError::Heading(value))
        }
    }

    fn from_degrees_wrapping(value: f64) -> Self {
        let wrapped = value.rem_euclid(360.0);
        // rem_euclid can yield exactly 360.0 for tiny negative inputs.
        Self(if wrapped >= 360.0 { 0.0 } else { wrapped })
    }

    /// Heading in degrees `[0, 360)`.
    pub fn as_degrees(self) -> f64 {
        self.0
    }
}

/// A non-negative length in metres.
#[derive(Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "f64", into = "f64"))]
pub struct Distance(f64);

impl Distance {
    /// Validate a distance in metres.
    pub fn metres(value: f64) -> Result<Self, ValidationError> {
        if value.is_finite() && value >= 0.0 {
            Ok(Self(value))
        } else {
            Err(ValidationError::Distance(value))
        }
    }

    /// Distance in metres.
    pub fn as_metres(self) -> f64 {
        self.0
    }
}

impl fmt::Display for Distance {
    /// Human-friendly: `238 m`, `1.4 km`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 1000.0 {
            write!(f, "{:.0} m", self.0)
        } else {
            write!(f, "{:.1} km", self.0 / 1000.0)
        }
    }
}

macro_rules! f64_newtype_impls {
    ($($ty:ident => $ctor:ident),* $(,)?) => {$(
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($ty), "({})"), self.0)
            }
        }
        impl TryFrom<f64> for $ty {
            type Error = ValidationError;
            fn try_from(v: f64) -> Result<Self, ValidationError> {
                Self::$ctor(v)
            }
        }
        impl From<$ty> for f64 {
            fn from(v: $ty) -> f64 {
                v.0
            }
        }
    )*};
}

f64_newtype_impls! {
    Latitude => new,
    Longitude => new,
    Altitude => hae,
    Speed => mps,
    Heading => degrees,
    Distance => metres,
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_out_of_range() {
        assert!(Latitude::new(90.01).is_err());
        assert!(Latitude::new(f64::NAN).is_err());
        assert!(Longitude::new(-180.01).is_err());
        assert!(Altitude::hae(f64::INFINITY).is_err());
        assert!(Speed::mps(-1.0).is_err());
        assert!(Precision::new(Some(-1.0), None).is_err());
    }

    #[test]
    fn heading_wraps() {
        assert_eq!(Heading::degrees(-90.0).unwrap().as_degrees(), 270.0);
        assert_eq!(Heading::degrees(720.0).unwrap().as_degrees(), 0.0);
        assert_eq!(Heading::degrees(-1e-20).unwrap().as_degrees(), 0.0);
    }

    #[test]
    fn haversine_known_distance() {
        // Washington Monument -> Lincoln Memorial, roughly 1.95 km.
        let a = GeoPoint::from_degrees(38.889_5, -77.035_3).unwrap();
        let b = GeoPoint::from_degrees(38.889_3, -77.050_2).unwrap();
        let d = a.distance_to(b).as_metres();
        assert!((1_250.0..1_350.0).contains(&d), "{d}");
        let brg = a.bearing_to(b).as_degrees();
        assert!((265.0..275.0).contains(&brg), "{brg}");
    }

    #[test]
    fn distance_display() {
        assert_eq!(Distance::metres(238.4).unwrap().to_string(), "238 m");
        assert_eq!(Distance::metres(1_400.0).unwrap().to_string(), "1.4 km");
    }

    proptest! {
        #[test]
        fn distance_is_symmetric_and_bounded(
            lat1 in -90.0f64..=90.0, lon1 in -180.0f64..=180.0,
            lat2 in -90.0f64..=90.0, lon2 in -180.0f64..=180.0,
        ) {
            let a = GeoPoint::from_degrees(lat1, lon1).unwrap();
            let b = GeoPoint::from_degrees(lat2, lon2).unwrap();
            let ab = a.distance_to(b).as_metres();
            let ba = b.distance_to(a).as_metres();
            prop_assert!((ab - ba).abs() < 1e-6);
            prop_assert!(ab <= EARTH_RADIUS_M * std::f64::consts::PI + 1e-6);
            let h = a.bearing_to(b).as_degrees();
            prop_assert!((0.0..360.0).contains(&h));
        }
    }
}
