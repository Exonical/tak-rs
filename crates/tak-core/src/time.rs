//! Timestamps and validity windows.

use std::fmt;
use std::ops::{Add, Sub};

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::ValidationError;

pub use time::Duration;

/// An instant in UTC with millisecond precision as used on the TAK wire.
///
/// Internally backed by [`time::OffsetDateTime`]; always normalised to UTC.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Timestamp(
    #[cfg_attr(feature = "serde", serde(with = "time::serde::rfc3339"))] OffsetDateTime,
);

impl Timestamp {
    /// The Unix epoch.
    pub const UNIX_EPOCH: Self = Self(OffsetDateTime::UNIX_EPOCH);

    /// Current wall-clock time.
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc())
    }

    /// Wrap an [`OffsetDateTime`], converting to UTC.
    pub fn from_datetime(dt: OffsetDateTime) -> Self {
        Self(dt.to_offset(time::UtcOffset::UTC))
    }

    /// Build from milliseconds since the Unix epoch.
    pub fn from_unix_millis(millis: i64) -> Result<Self, ValidationError> {
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(millis) * 1_000_000)
            .map(Self)
            .map_err(|e| ValidationError::Timestamp(e.to_string()))
    }

    /// Milliseconds since the Unix epoch (truncating sub-millisecond precision).
    pub fn unix_millis(self) -> i64 {
        (self.0.unix_timestamp_nanos() / 1_000_000) as i64
    }

    /// Parse an RFC 3339 / ISO 8601 timestamp such as `2024-05-01T12:00:00.000Z`.
    pub fn parse_rfc3339(s: &str) -> Result<Self, ValidationError> {
        OffsetDateTime::parse(s, &Rfc3339)
            .map(Self::from_datetime)
            .map_err(|e| ValidationError::Timestamp(format!("{s:?}: {e}")))
    }

    /// Format as the CoT canonical form `YYYY-MM-DDTHH:MM:SS.mmmZ`.
    pub fn to_cot_string(self) -> String {
        let t = self.0;
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            t.year(),
            u8::from(t.month()),
            t.day(),
            t.hour(),
            t.minute(),
            t.second(),
            t.millisecond()
        )
    }

    /// Access the underlying [`OffsetDateTime`].
    pub fn as_datetime(self) -> OffsetDateTime {
        self.0
    }

    /// Checked addition; `None` on overflow.
    pub fn checked_add(self, d: Duration) -> Option<Self> {
        self.0.checked_add(d).map(Self)
    }

    /// Saturating duration since `earlier` (zero if `earlier` is later).
    pub fn saturating_since(self, earlier: Timestamp) -> Duration {
        if earlier > self {
            Duration::ZERO
        } else {
            self.0 - earlier.0
        }
    }
}

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Timestamp({})", self.to_cot_string())
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_cot_string())
    }
}

impl Add<Duration> for Timestamp {
    type Output = Timestamp;

    /// Saturating at the representable range of [`OffsetDateTime`].
    fn add(self, rhs: Duration) -> Self::Output {
        self.0.checked_add(rhs).map_or_else(
            || {
                if rhs.is_negative() {
                    Self(OffsetDateTime::new_utc(
                        time::Date::MIN,
                        time::Time::MIDNIGHT,
                    ))
                } else {
                    Self(OffsetDateTime::new_utc(
                        time::Date::MAX,
                        time::Time::MIDNIGHT,
                    ))
                }
            },
            Self,
        )
    }
}

impl Sub<Timestamp> for Timestamp {
    type Output = Duration;

    fn sub(self, rhs: Timestamp) -> Duration {
        self.0 - rhs.0
    }
}

impl From<OffsetDateTime> for Timestamp {
    fn from(dt: OffsetDateTime) -> Self {
        Self::from_datetime(dt)
    }
}

/// The CoT validity triple: when the event was generated, when it becomes
/// valid and when it should be considered stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Validity {
    /// Generation time of the event.
    pub time: Timestamp,
    /// Start of the validity interval.
    pub start: Timestamp,
    /// End of the validity interval.
    pub stale: Timestamp,
}

impl Validity {
    /// Construct a validity window, rejecting `start > stale`.
    pub fn new(
        time: Timestamp,
        start: Timestamp,
        stale: Timestamp,
    ) -> Result<Self, ValidationError> {
        if start > stale {
            return Err(ValidationError::InvertedValidity {
                start: start.to_string(),
                stale: stale.to_string(),
            });
        }
        Ok(Self { time, start, stale })
    }

    /// A window starting at `now` and lasting `ttl`.
    pub fn from_now(now: Timestamp, ttl: Duration) -> Self {
        Self {
            time: now,
            start: now,
            stale: now + ttl,
        }
    }

    /// Whether the window has expired at `now`.
    pub fn is_stale_at(&self, now: Timestamp) -> bool {
        now >= self.stale
    }

    /// Whether `now` lies within `[start, stale)`.
    pub fn is_active_at(&self, now: Timestamp) -> bool {
        now >= self.start && now < self.stale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cot_timestamps() {
        let t = Timestamp::parse_rfc3339("2024-05-01T12:34:56.789Z").unwrap();
        assert_eq!(t.to_cot_string(), "2024-05-01T12:34:56.789Z");
        let t = Timestamp::parse_rfc3339("2024-05-01T12:34:56Z").unwrap();
        assert_eq!(t.to_cot_string(), "2024-05-01T12:34:56.000Z");
        let t = Timestamp::parse_rfc3339("2024-05-01T14:34:56+02:00").unwrap();
        assert_eq!(t.to_cot_string(), "2024-05-01T12:34:56.000Z");
        assert!(Timestamp::parse_rfc3339("yesterday").is_err());
    }

    #[test]
    fn millis_roundtrip() {
        let t = Timestamp::from_unix_millis(1_714_566_896_789).unwrap();
        assert_eq!(t.unix_millis(), 1_714_566_896_789);
        assert_eq!(t.to_cot_string(), "2024-05-01T12:34:56.789Z");
    }

    #[test]
    fn validity_rules() {
        let now = Timestamp::from_unix_millis(1_000_000).unwrap();
        let v = Validity::from_now(now, Duration::seconds(60));
        assert!(!v.is_stale_at(now));
        assert!(v.is_stale_at(now + Duration::seconds(60)));
        assert!(Validity::new(now, now + Duration::seconds(1), now).is_err());
    }

    #[test]
    fn add_saturates() {
        let t = Timestamp::UNIX_EPOCH + Duration::seconds(i64::MAX / 4);
        assert!(t > Timestamp::UNIX_EPOCH);
    }
}
