//! Validation errors produced by domain type constructors.

/// Error returned when a value does not satisfy the invariants of a domain type.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    /// Latitude outside `[-90, 90]` or not finite.
    #[error("latitude {0} is out of range [-90, 90]")]
    Latitude(f64),
    /// Longitude outside `[-180, 180]` or not finite.
    #[error("longitude {0} is out of range [-180, 180]")]
    Longitude(f64),
    /// Altitude is not a finite number.
    #[error("altitude {0} is not finite")]
    Altitude(f64),
    /// Negative or non-finite error estimate (CE/LE).
    #[error("precision {0} must be finite and non-negative")]
    Precision(f64),
    /// Negative or non-finite speed.
    #[error("speed {0} m/s must be finite and non-negative")]
    Speed(f64),
    /// Non-finite heading.
    #[error("heading {0} must be finite")]
    Heading(f64),
    /// Negative or non-finite distance.
    #[error("distance {0} m must be finite and non-negative")]
    Distance(f64),
    /// Non-positive or non-finite shape dimension (radius, axis length).
    #[error("shape dimension {0} m must be finite and positive")]
    ShapeDimension(f64),
    /// A geometry that requires vertices was given too few.
    #[error("{geometry} requires at least {min} vertices, got {got}")]
    TooFewVertices {
        /// Geometry kind name.
        geometry: &'static str,
        /// Minimum vertex count.
        min: usize,
        /// Actual vertex count.
        got: usize,
    },
    /// Empty UID.
    #[error("uid must not be empty")]
    EmptyUid,
    /// UID contains characters that are not allowed or is too long.
    #[error("uid {0:?} is invalid")]
    Uid(String),
    /// Empty callsign.
    #[error("callsign must not be empty")]
    EmptyCallsign,
    /// Callsign contains control characters or is too long.
    #[error("callsign {0:?} is invalid")]
    Callsign(String),
    /// CoT type string does not follow the dash-separated grammar.
    #[error("cot type {0:?} is invalid")]
    CotType(String),
    /// CoT `how` string does not follow the dash-separated grammar.
    #[error("how {0:?} is invalid")]
    How(String),
    /// A timestamp could not be interpreted.
    #[error("timestamp is invalid: {0}")]
    Timestamp(String),
    /// Time ordering invariant violated (`start` after `stale`).
    #[error("validity window is inverted: start {start} is after stale {stale}")]
    InvertedValidity {
        /// Start of validity.
        start: String,
        /// End of validity.
        stale: String,
    },
    /// Generic text field is empty or contains control characters.
    #[error("{field} is invalid: {reason}")]
    Text {
        /// Name of the field.
        field: &'static str,
        /// Why validation failed.
        reason: &'static str,
    },
}

pub(crate) fn has_control_chars(s: &str) -> bool {
    s.chars().any(char::is_control)
}
