use std::collections::HashMap;
use std::sync::Arc;
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::permission::EntityType;

mod builtin;
mod effect;
mod engine;
mod parser;
mod snapshot;
mod step;

pub use builtin::execute_builtin;
pub use effect::Effect;
pub use engine::{
    execute_command, resume_from_snapshot, CommandRuntime, ExecutionResult, ResumeAction,
    StepControl, StepRun, MAX_COMMAND_STEPS,
};
pub use snapshot::{CmdState, ExecutionSnapshot, InputGate, Invocation, Phase};
pub use step::{CommandSpec, Step};

static GLOBAL_COMMAND_REGISTRY: OnceLock<Arc<CommandRegistry>> = OnceLock::new();
static GLOBAL_TASK_RUNTIME: RwLock<Option<Arc<dyn crate::task_context::AgentTaskRuntime>>> =
    RwLock::new(None);

pub fn set_global_command_registry(registry: Arc<CommandRegistry>) -> Result<()> {
    GLOBAL_COMMAND_REGISTRY
        .set(registry)
        .map_err(|_| crate::string_error!("command registry already initialized"))
}

pub fn set_global_task_runtime(runtime: Arc<dyn crate::task_context::AgentTaskRuntime>) {
    *GLOBAL_TASK_RUNTIME.write().unwrap() = Some(runtime);
}

pub fn global_task_runtime() -> Option<Arc<dyn crate::task_context::AgentTaskRuntime>> {
    GLOBAL_TASK_RUNTIME.read().unwrap().clone()
}

pub fn global_command_registry() -> Option<Arc<CommandRegistry>> {
    GLOBAL_COMMAND_REGISTRY.get().cloned()
}

pub fn build_help_text() -> Option<String> {
    let registry = global_command_registry()?;
    let mut lines = registry
        .list_commands()
        .iter()
        .map(|spec| {
            let aliases = if spec.aliases.is_empty() {
                String::new()
            } else {
                format!(" (别名: {})", spec.aliases.join(", "))
            };
            format!("/{} — {}{}", spec.name, spec.description, aliases)
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push("暂无可用命令。".to_string());
    }
    Some(lines.join("\n"))
}

/// Defines which agent types a command is available for. `agent_type` is the
/// RoleService kind tag (`qq_chat` / `workspace`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandScope {
    All,
    QqChat,
    Workspace,
    /// Legacy, not produced by any current runtime.
    HttpStream,
    Specific {
        agent_ids: Vec<String>,
    },
}

impl Default for CommandScope {
    fn default() -> Self {
        Self::All
    }
}

impl CommandScope {
    pub fn label(&self) -> &str {
        match self {
            Self::All => "全部",
            Self::QqChat => "QQ Chat",
            Self::Workspace => "Workspace",
            Self::HttpStream => "HTTP Stream",
            Self::Specific { .. } => "指定 Agent",
        }
    }

    pub fn matches(&self, agent_type: &str, agent_id: &str) -> bool {
        match self {
            Self::All => true,
            Self::QqChat => agent_type == "qq_chat",
            Self::Workspace => agent_type == "workspace",
            Self::HttpStream => agent_type == "http_stream",
            Self::Specific { agent_ids } => agent_ids.iter().any(|id| id == agent_id),
        }
    }
}

/// Identifies the source of a command invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandChannel {
    QqChat {
        sender_id: String,
        is_group: bool,
        group_id: Option<String>,
        target_id: String,
    },
    HttpStream {
        api_key: String,
    },
    DashboardChat {
        session_id: Option<String>,
    },
}

/// Context passed to command matching and permission checks, and carried
/// inside an invocation snapshot so a paused command resumes against the
/// original request context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandContext {
    pub agent_type: String,
    pub agent_id: String,
    pub caller_id: String,
    pub channel: CommandChannel,
    /// Permission group of the invoking RoleService. `None` keeps the legacy open gate used by
    /// the dashboard.
    #[serde(default)]
    pub permission_group: Option<String>,
    /// Entity type of `caller_id`; `None` keeps the legacy open gate.
    #[serde(default)]
    pub entity_type: Option<EntityType>,
}

#[derive(Debug, Clone)]
pub struct PermissionCheckResult {
    pub matched: bool,
    pub allowed: bool,
}

// CommandRegistry — central command spec router and permission manager.
//
// ## Purpose
//
// CommandRegistry owns the lifecycle of slash-command *definitions*: registration,
// permission management, listing, and name/scope matching. It does not execute
// commands — execution is a per-channel engine run over a [`CommandSpec`].
//
// ## Design
//
// - **Registration** (`register`) accepts a `CommandSpec` (pure data).
// - **Lookup path** (`spec_for`): raw input is tested for a leading `/`, then the
//   command name (case-insensitive) is matched first by primary name and then by
//   aliases. Scope is checked before permissions, so a command that does not apply
//   to the current agent type is silently ignored.
// - **Permissions** are stored in the system database permission groups and checked
//   through [`crate::permission::check_command_permission`].

struct CommandEntry {
    spec: Arc<CommandSpec>,
}

pub struct CommandRegistry {
    commands: HashMap<String, CommandEntry>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self { commands: HashMap::new() }
    }

    /// Register a command spec by its primary name.
    pub fn register(&mut self, spec: CommandSpec) {
        let name = spec.name.clone();
        self.commands.insert(name, CommandEntry { spec: Arc::new(spec) });
    }

    /// Look up a registered spec by canonical primary name (case-sensitive
    /// canonical names only — used for resume targets).
    pub fn get(&self, name: &str) -> Option<Arc<CommandSpec>> {
        self.commands.get(name).map(|entry| Arc::clone(&entry.spec))
    }

    /// Resolve a raw message to a spec plus parsed arguments, gated by scope.
    /// Permission is checked separately by the caller (or the engine) via
    /// [`CommandRegistry::check_permission`].
    pub fn spec_for(
        &self,
        ctx: &CommandContext,
        raw_input: &str,
    ) -> Option<(Arc<CommandSpec>, parser::ParsedCommand)> {
        let (entry, parsed) = self.find_matching_entry(ctx, raw_input)?;
        Some((Arc::clone(&entry.spec), parsed))
    }

    /// List all registered command specs (read-only metadata; stable order not
    /// guaranteed).
    pub fn list_commands(&self) -> Vec<&CommandSpec> {
        self.commands.values().map(|entry| entry.spec.as_ref()).collect()
    }

    /// Preview a command without executing it. Used by busy-session steer to
    /// decide whether a command may bypass the steer queue.
    pub fn preview<'a>(
        &'a self,
        ctx: &CommandContext,
        raw_input: &str,
    ) -> Option<CommandPreview<'a>> {
        let (spec, parsed) = self.find_matching_entry(ctx, raw_input)?;
        Some(CommandPreview {
            spec: spec.spec.as_ref(),
            args: parsed.args,
            passthrough_text: parsed.passthrough_text,
        })
    }

    pub fn check_permission(&self, ctx: &CommandContext, raw_input: &str) -> PermissionCheckResult {
        let Some((entry, _)) = self.find_matching_entry(ctx, raw_input) else {
            return PermissionCheckResult { matched: false, allowed: false };
        };

        let (Some(entity_type), Some(group)) = (ctx.entity_type, ctx.permission_group.as_deref())
        else {
            return PermissionCheckResult { matched: true, allowed: true };
        };
        let allowed = match crate::permission::check_command_permission(
            group,
            &ctx.caller_id,
            entity_type,
            &entry.spec.name,
        ) {
            Ok(allowed) => allowed,
            Err(error) => {
                log::warn!("failed to check permission for command {}: {error}", entry.spec.name);
                false
            }
        };
        PermissionCheckResult { matched: true, allowed }
    }

    fn find_matching_entry<'a>(
        &'a self,
        ctx: &CommandContext,
        raw_input: &str,
    ) -> Option<(&'a CommandEntry, parser::ParsedCommand)> {
        let trimmed = raw_input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let body = &trimmed[1..];
        let command_name = body.split_whitespace().next()?.to_lowercase();

        let entry = self.commands.get(&command_name).or_else(|| {
            self.commands.values().find(|entry| {
                entry.spec.aliases.iter().any(|a| a.eq_ignore_ascii_case(&command_name))
            })
        })?;

        if !entry.spec.scope.matches(&ctx.agent_type, &ctx.agent_id) {
            return None;
        }

        let parsed = parser::parse_command(raw_input, entry.spec.accepted_arg_count)?;
        Some((entry, parsed))
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed command preview used by callers that need to inspect a command
/// before deciding whether to execute it.
pub struct CommandPreview<'a> {
    pub spec: &'a CommandSpec,
    pub args: Vec<String>,
    pub passthrough_text: Option<String>,
}

/// Look up the spec that should resolve a resume target. Resolves aliases via
/// the registry so `/auth` resume targets may use either canonical or alias
/// names.
pub fn resolve_spec(name: &str) -> Option<Arc<CommandSpec>> {
    let registry = global_command_registry()?;
    if let Some(spec) = registry.get(name) {
        return Some(spec);
    }
    registry
        .list_commands()
        .into_iter()
        .find(|spec| {
            spec.name.eq_ignore_ascii_case(name)
                || spec.aliases.iter().any(|alias| alias.eq_ignore_ascii_case(name))
        })
        .map(|spec| Arc::new(spec.clone()))
}
