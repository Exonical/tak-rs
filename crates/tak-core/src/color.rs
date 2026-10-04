//! Colours as carried by TAK (`<color argb="-1"/>`, `<strokeColor value="..."/>`).

use std::fmt;

/// A 32-bit ARGB colour.
///
/// TAK encodes colours as a *signed* 32-bit integer (Java `int`); use
/// [`Argb::from_i32`] / [`Argb::to_i32`] at the wire boundary.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(from = "u32", into = "u32"))]
pub struct Argb(u32);

impl Argb {
    /// Opaque white (`-1` on the wire), the TAK default.
    pub const WHITE: Self = Self(0xFFFF_FFFF);

    /// Build from components.
    pub const fn new(a: u8, r: u8, g: u8, b: u8) -> Self {
        Self(u32::from_be_bytes([a, r, g, b]))
    }

    /// Interpret a Java-style signed 32-bit ARGB value.
    pub const fn from_i32(value: i32) -> Self {
        Self(value as u32)
    }

    /// Java-style signed 32-bit ARGB value.
    pub const fn to_i32(self) -> i32 {
        self.0 as i32
    }

    /// Raw packed `0xAARRGGBB`.
    pub const fn to_u32(self) -> u32 {
        self.0
    }

    /// Alpha component.
    pub const fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }

    /// Red component.
    pub const fn red(self) -> u8 {
        (self.0 >> 16) as u8
    }

    /// Green component.
    pub const fn green(self) -> u8 {
        (self.0 >> 8) as u8
    }

    /// Blue component.
    pub const fn blue(self) -> u8 {
        self.0 as u8
    }
}

impl From<u32> for Argb {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<Argb> for u32 {
    fn from(c: Argb) -> Self {
        c.0
    }
}

impl fmt::Debug for Argb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Argb(#{:08X})", self.0)
    }
}

impl fmt::Display for Argb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:08X}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_int_roundtrip() {
        assert_eq!(Argb::from_i32(-1), Argb::WHITE);
        assert_eq!(Argb::WHITE.to_i32(), -1);
        let c = Argb::new(0x80, 0x10, 0x20, 0x30);
        assert_eq!(Argb::from_i32(c.to_i32()), c);
        assert_eq!(
            (c.alpha(), c.red(), c.green(), c.blue()),
            (0x80, 0x10, 0x20, 0x30)
        );
        assert_eq!(c.to_string(), "#80102030");
    }
}
