use serde::{Deserialize, Serialize};

use super::effect::Effect;
use super::CommandContext;

/// Serializable cross-step shared context. Steps read and write through it so
/// parameters can flow between setup, conditions and body (and across a pause /
/// resume boundary).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CmdState {
    #[serde(default)]
    pub values: serde_json::Map<String, serde_json::Value>,
}

impl CmdState {
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.values.get(key)
    }

    pub fn insert(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.values.insert(key.into(), value);
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(serde_json::Value::as_str)
    }
}

/// Serializable invocation facts for one command run (or resume).
///
/// `ctx` mirrors the channel's [`CommandContext`] and is the part that lets a
/// paused run be resumed against the original sender/target even when the
/// resume message arrives from a different context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Invocation {
    pub ctx: CommandContext,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub passthrough: Option<String>,
    /// Whether this run already passed setup (a resume). Not persisted meaningfully —
    /// the engine sets it true when loading a snapshot.
    #[serde(default)]
    pub resumed: bool,
}

/// Which phase of a [`CommandSpec`](super::CommandSpec) is executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Setup,
    Conditions,
    Body,
}

impl Phase {
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Setup => Some(Self::Conditions),
            Self::Conditions => Some(Self::Body),
            Self::Body => None,
        }
    }
}

/// External input a command must wait for before continuing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputGate {
    NeedsAuth { purpose: String },
    // Reserved for future custom-command / multi-step skill flows.
    // NeedsConfirm { prompt: String },
    // NeedsParam { question: String },
}

/// A fully serializable pause point. Persisted by the channel (e.g. the QQ
/// privilege-auth row) so a later message can resume the exact remaining steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionSnapshot {
    pub version: u32,
    /// The spec whose phases are running (resolved again on resume).
    pub spec_name: String,
    pub invocation: Invocation,
    pub phase: Phase,
    /// Cursor into the current phase's step list. On a pause it points at the
    /// step that requested the input so the guard re-runs after resume.
    pub step_index: usize,
    /// Expanded sub-steps not yet consumed (LIFO).
    #[serde(default)]
    pub expanded: Vec<super::Step>,
    pub state: CmdState,
    #[serde(default)]
    pub pending_effects: Vec<Effect>,
    pub gate: InputGate,
}

impl ExecutionSnapshot {
    pub const CURRENT_VERSION: u32 = 1;
}
