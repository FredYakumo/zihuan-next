use serde::{Deserialize, Serialize};

/// A coarse-grained unit used to express a span of time.
///
/// Shared across feature crates that translate a configured window or interval
/// (for example a rate-limit window) into a concrete number of seconds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimeWindowUnit {
    Minute,
    Hour,
    Day,
}

impl TimeWindowUnit {
    /// Canonical snake_case identifier, used for persistence keys and text values.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
        }
    }

    /// Number of seconds contained in a single unit.
    pub fn seconds(self) -> i64 {
        match self {
            Self::Minute => 60,
            Self::Hour => 60 * 60,
            Self::Day => 60 * 60 * 24,
        }
    }
}
