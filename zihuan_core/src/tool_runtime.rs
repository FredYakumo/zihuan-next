use serde::{Deserialize, Serialize};

/// Declares whether a Brain tool should be treated as short or long running.
///
/// `Short` tools execute inline without task lifecycle notifications.
/// `Long` tools opt into the tracked-task flow so callers can surface
/// progress and completion messages while still waiting for the real result.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ToolRunDuration {
    #[default]
    Short,
    Long,
}