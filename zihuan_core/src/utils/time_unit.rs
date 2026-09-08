use serde::{Deserialize, Deserializer, Serialize};

/// A coarse-grained unit used to express a span of time.
///
/// Shared across feature crates that translate a configured window or interval
/// (for example a rate-limit window or a periodic task interval) into a concrete
/// number of seconds.
///
/// Serialized and stored in the canonical singular form (`minute`/`hour`/`day`).
/// Deserialization also accepts the legacy plural spellings
/// (`minutes`/`hours`/`days`) so older stored configs keep loading.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    #[default]
    Minute,
    Hour,
    Day,
}

impl TimeUnit {
    /// Canonical singular snake_case identifier, used for persistence keys and text values.
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

impl<'de> Deserialize<'de> for TimeUnit {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        match text.trim().to_ascii_lowercase().as_str() {
            "minute" | "minutes" => Ok(Self::Minute),
            "hour" | "hours" => Ok(Self::Hour),
            "day" | "days" => Ok(Self::Day),
            _ => Err(serde::de::Error::custom(format!(
                "unknown time unit '{text}', expected minute/minutes, hour/hours, or day/days"
            ))),
        }
    }
}
