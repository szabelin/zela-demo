//! Region definitions for Zela server locations.
//!
//! The four Zela regions represent geographic locations where
//! Zela deploys infrastructure for low-latency Solana access.

use serde::Serialize;

/// The four Zela server regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Region {
    Frankfurt,
    Dubai,
    NewYork,
    Tokyo,
}

impl Region {
    /// Default fallback region for unknown validator locations.
    ///
    /// Frankfurt is chosen because:
    /// - ~38% of Solana validators are in Europe
    /// - Central position minimizes worst-case latency
    pub const DEFAULT: Region = Region::Frankfurt;

    /// Human-readable geographic label for the region.
    pub fn geo_label(&self) -> &'static str {
        match self {
            Region::Frankfurt => "Europe/Frankfurt",
            Region::Dubai => "Middle East/Dubai",
            Region::NewYork => "North America/New York",
            Region::Tokyo => "Asia/Tokyo",
        }
    }
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Region::Frankfurt => write!(f, "Frankfurt"),
            Region::Dubai => write!(f, "Dubai"),
            Region::NewYork => write!(f, "NewYork"),
            Region::Tokyo => write!(f, "Tokyo"),
        }
    }
}

impl From<u8> for Region {
    fn from(value: u8) -> Self {
        match value {
            0 => Region::Frankfurt,
            1 => Region::Dubai,
            2 => Region::NewYork,
            3 => Region::Tokyo,
            _ => Region::DEFAULT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_display() {
        assert_eq!(Region::Frankfurt.to_string(), "Frankfurt");
        assert_eq!(Region::Dubai.to_string(), "Dubai");
        assert_eq!(Region::NewYork.to_string(), "NewYork");
        assert_eq!(Region::Tokyo.to_string(), "Tokyo");
    }

    #[test]
    fn test_geo_label() {
        assert_eq!(Region::Frankfurt.geo_label(), "Europe/Frankfurt");
        assert_eq!(Region::Tokyo.geo_label(), "Asia/Tokyo");
    }

    #[test]
    fn test_from_u8() {
        assert_eq!(Region::from(0), Region::Frankfurt);
        assert_eq!(Region::from(1), Region::Dubai);
        assert_eq!(Region::from(2), Region::NewYork);
        assert_eq!(Region::from(3), Region::Tokyo);
        assert_eq!(Region::from(99), Region::DEFAULT);
    }
}
