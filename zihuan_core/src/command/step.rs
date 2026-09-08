use serde::{Deserialize, Serialize};

use super::CommandScope;

/// One {指令, 描述, 参数} record.
///
/// Commands, preconditions and side effects all use the same carrier: `op`
/// selects an executor owned by the channel runtime (builtin://*, ims://*, ...),
/// `desc` is human/LLM-facing text, and `params` carries structured operands.
/// Everything is plain data so a step can cross the message boundary (persisted
/// inside an [`ExecutionSnapshot`](super::ExecutionSnapshot)) and later be
/// forwarded to future custom-command backends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub op: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

impl Step {
    pub fn new(op: impl Into<String>) -> Self {
        Self { op: op.into(), desc: String::new(), params: serde_json::Value::Null }
    }

    pub fn with_params(
        op: impl Into<String>,
        desc: impl Into<String>,
        params: serde_json::Value,
    ) -> Self {
        Self { op: op.into(), desc: desc.into(), params }
    }
}

/// Declarative description of a command. Registered in the global
/// [`CommandRegistry`](super::CommandRegistry); the registry no longer owns
/// executable code — a channel runtime executes the steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub scope: CommandScope,
    /// Number of positional arguments consumed after the command name
    /// (0 = whole remainder is passthrough), matching the parser semantics.
    #[serde(default)]
    pub accepted_arg_count: u8,
    /// Whether the command may bypass steer queueing while another reply flow
    /// is active. Defaults to false.
    #[serde(default)]
    pub allow_steer_bypass: bool,
    /// Steps run once when the command starts (e.g. creating a waiting-auth
    /// task). Skipped when a run resumes after an input gate.
    #[serde(default)]
    pub setup: Vec<Step>,
    /// Preconditions. When a step returns [`StepRun::RequireInput`], execution
    /// pauses at that condition and resumes after the gate is satisfied.
    #[serde(default)]
    pub conditions: Vec<Step>,
    /// The side-effect queue executed once preconditions pass.
    #[serde(default)]
    pub body: Vec<Step>,
}

impl CommandSpec {
    /// Shorthand for specs whose preconditions and body are plain steps and no
    /// setup is needed.
    pub fn simple(
        name: impl Into<String>,
        description: impl Into<String>,
        scope: CommandScope,
        accepted_arg_count: u8,
        allow_steer_bypass: bool,
        body: Vec<Step>,
    ) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            description: description.into(),
            scope,
            accepted_arg_count,
            allow_steer_bypass,
            setup: Vec::new(),
            conditions: Vec::new(),
            body,
        }
    }
}
