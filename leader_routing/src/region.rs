use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Region {
    Frankfurt,
    Dubai,
    NewYork,
    Tokyo,
    Unknown,
}

impl Region {
    pub fn geo_label(&self) -> &'static str {
        match self {
            Region::Frankfurt => "Europe/Frankfurt",
            Region::Dubai => "Middle East/Dubai",
            Region::NewYork => "North America/New York",
            Region::Tokyo => "Asia/Tokyo",
            Region::Unknown => "UNKNOWN",
        }
    }

    pub fn routing_destination(&self) -> Region {
        match self {
            Region::Unknown => Region::Frankfurt,
            other => *other,
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
            Region::Unknown => write!(f, "Unknown"),
        }
    }
}

impl From<u8> for Region {
    fn from(v: u8) -> Self {
        match v {
            0 => Region::Frankfurt,
            1 => Region::Dubai,
            2 => Region::NewYork,
            3 => Region::Tokyo,
            _ => Region::Unknown,
        }
    }
}
