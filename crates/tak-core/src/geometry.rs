//! Shapes a TAK object can have on the map.

use crate::error::ValidationError;
use crate::geo::GeoPoint;

/// Geometry of a TAK object.
///
/// Every geometry has an *anchor* point which is what CoT carries in
/// `<point>`; richer shapes are reconstructed from detail extensions.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
pub enum Geometry {
    /// A single position.
    Point(GeoPoint),
    /// An open path of two or more vertices.
    Polyline(Vec<GeoPoint>),
    /// A closed ring of three or more vertices (first vertex is not repeated).
    Polygon(Vec<GeoPoint>),
    /// A circle.
    Circle {
        /// Centre.
        center: GeoPoint,
        /// Radius in metres.
        radius_m: f64,
    },
    /// A rectangle given by its four corners in ring order.
    Rectangle {
        /// Corners in ring order.
        corners: [GeoPoint; 4],
    },
    /// An ellipse.
    Ellipse {
        /// Centre.
        center: GeoPoint,
        /// Full length of the major axis in metres.
        major_m: f64,
        /// Full length of the minor axis in metres.
        minor_m: f64,
        /// Rotation of the major axis, degrees clockwise from north.
        angle_deg: f64,
    },
}

fn positive_dimension(v: f64) -> Result<f64, ValidationError> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(ValidationError::ShapeDimension(v))
    }
}

impl Geometry {
    /// Construct a polyline, requiring at least two vertices.
    pub fn polyline(vertices: Vec<GeoPoint>) -> Result<Self, ValidationError> {
        if vertices.len() < 2 {
            return Err(ValidationError::TooFewVertices {
                geometry: "polyline",
                min: 2,
                got: vertices.len(),
            });
        }
        Ok(Self::Polyline(vertices))
    }

    /// Construct a polygon, requiring at least three vertices.
    pub fn polygon(vertices: Vec<GeoPoint>) -> Result<Self, ValidationError> {
        if vertices.len() < 3 {
            return Err(ValidationError::TooFewVertices {
                geometry: "polygon",
                min: 3,
                got: vertices.len(),
            });
        }
        Ok(Self::Polygon(vertices))
    }

    /// Construct a circle with a positive radius.
    pub fn circle(center: GeoPoint, radius_m: f64) -> Result<Self, ValidationError> {
        Ok(Self::Circle {
            center,
            radius_m: positive_dimension(radius_m)?,
        })
    }

    /// Construct an ellipse with positive axes and a finite angle.
    pub fn ellipse(
        center: GeoPoint,
        major_m: f64,
        minor_m: f64,
        angle_deg: f64,
    ) -> Result<Self, ValidationError> {
        if !angle_deg.is_finite() {
            return Err(ValidationError::Heading(angle_deg));
        }
        Ok(Self::Ellipse {
            center,
            major_m: positive_dimension(major_m)?,
            minor_m: positive_dimension(minor_m)?,
            angle_deg: angle_deg.rem_euclid(360.0),
        })
    }

    /// The representative position carried in a CoT `<point>`.
    ///
    /// For points, circles and ellipses this is the centre; for paths it is
    /// the first vertex; for rectangles it is the centroid of the corners.
    pub fn anchor(&self) -> GeoPoint {
        match self {
            Self::Point(p) | Self::Circle { center: p, .. } | Self::Ellipse { center: p, .. } => *p,
            Self::Polyline(v) | Self::Polygon(v) => v.first().copied().unwrap_or(GeoPoint::ORIGIN),
            Self::Rectangle { corners } => centroid(corners).unwrap_or(corners[0]),
        }
    }

    /// Vertices for path-like geometries (`None` for point/circle/ellipse).
    pub fn vertices(&self) -> Option<&[GeoPoint]> {
        match self {
            Self::Polyline(v) | Self::Polygon(v) => Some(v),
            Self::Rectangle { corners } => Some(corners),
            _ => None,
        }
    }

    /// Whether this geometry is a single point.
    pub fn is_point(&self) -> bool {
        matches!(self, Self::Point(_))
    }
}

fn centroid(points: &[GeoPoint]) -> Option<GeoPoint> {
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f64;
    let lat = points.iter().map(|p| p.lat.degrees()).sum::<f64>() / n;
    let lon = points.iter().map(|p| p.lon.degrees()).sum::<f64>() / n;
    GeoPoint::from_degrees(lat, lon).ok()
}

impl From<GeoPoint> for Geometry {
    fn from(p: GeoPoint) -> Self {
        Self::Point(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint::from_degrees(lat, lon).unwrap()
    }

    #[test]
    fn constructors_validate() {
        assert!(Geometry::polyline(vec![p(0.0, 0.0)]).is_err());
        assert!(Geometry::polygon(vec![p(0.0, 0.0), p(1.0, 1.0)]).is_err());
        assert!(Geometry::circle(p(0.0, 0.0), 0.0).is_err());
        assert!(Geometry::ellipse(p(0.0, 0.0), 10.0, 5.0, f64::NAN).is_err());
        assert!(Geometry::ellipse(p(0.0, 0.0), 10.0, 5.0, -90.0).is_ok());
    }

    #[test]
    fn anchors() {
        assert_eq!(Geometry::Point(p(1.0, 2.0)).anchor(), p(1.0, 2.0));
        assert_eq!(
            Geometry::polyline(vec![p(1.0, 2.0), p(3.0, 4.0)])
                .unwrap()
                .anchor(),
            p(1.0, 2.0)
        );
        let rect = Geometry::Rectangle {
            corners: [p(0.0, 0.0), p(0.0, 2.0), p(2.0, 2.0), p(2.0, 0.0)],
        };
        assert_eq!(rect.anchor(), p(1.0, 1.0));
    }
}
