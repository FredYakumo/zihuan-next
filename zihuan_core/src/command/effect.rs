use serde::{Deserialize, Serialize};

/// A channel-neutral effect produced by command steps. The channel runtime
/// decides how to render each effect (QQ: direct reply/forward; Dashboard:
/// assistant message/SSE). All values are serializable so queued effects can be
/// persisted inside an execution snapshot and delivered after a resume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// Primary visible text. QQ = direct reply bubble; Dashboard = assistant message.
    Text(String),
    /// Lightweight notice. QQ = separate bubble; Dashboard = rendered like Text
    /// (channel policy).
    Notice(String),
    /// Long content. QQ = forward message; Dashboard = assistant text.
    Forward(String),
    /// Ask the channel to start a fresh conversation (clear history / new session).
    StartNewConversation,
    /// Write a shared context value (cross-step parameter passing).
    SetContext { key: String, value: serde_json::Value },
}
