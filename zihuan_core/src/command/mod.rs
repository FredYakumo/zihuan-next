use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::validation_error;

mod parser;

/// Defines which agent types a command is available for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandScope {
    All,
    QqChat,
    HttpStream,
    Specific { agent_ids: Vec<String> },
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
            Self::HttpStream => "HTTP Stream",
            Self::Specific { .. } => "指定 Agent",
        }
    }

    pub fn matches(&self, agent_type: &str, _agent_id: &str) -> bool {
        match self {
            Self::All => true,
            Self::QqChat => agent_type == "qq_chat",
            Self::HttpStream => agent_type == "http_stream",
            Self::Specific { agent_ids } => agent_ids.iter().any(|id| id == _agent_id),
        }
    }
}

/// A registered command definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub scope: CommandScope,
    /// Number of positional arguments to consume after the command name.
    /// 0 = parameterless: all remaining text is passthrough for the LLM.
    /// N = consume up to N tokens as args, remainder is passthrough.
    #[serde(default)]
    pub accepted_arg_count: u8,
    /// Whether this command may bypass steer queueing while another reply
    /// flow is active. Defaults to false so only explicitly safe commands opt in.
    #[serde(default)]
    pub allow_steer_bypass: bool,
}

/// Permission rules that control who can use a command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "rule_type", rename_all = "snake_case")]
pub enum PermissionRule {
    Everyone,
    QqUsers {
        allowed_ids: Vec<String>,
    },
    ApiKeys {
        allowed_keys: Vec<String>,
    },
    Custom {
        custom_type: String,
        allow_list: Vec<String>,
    },
}

/// Stored permission binding for a command (persisted via config system).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPermission {
    pub command_name: String,
    pub rules: Vec<PermissionRule>,
    pub enabled: bool,
}

/// Identifies the source of a command invocation.
#[derive(Debug, Clone)]
pub enum CommandChannel {
    QqChat {
        sender_id: String,
        is_group: bool,
        group_id: Option<i64>,
        target_id: String,
    },
    HttpStream {
        api_key: String,
    },
    DashboardChat {
        session_id: Option<String>,
    },
}

/// Context passed to command dispatch and permission checks.
#[derive(Debug, Clone)]
pub struct CommandContext {
    pub agent_type: String,
    pub agent_id: String,
    pub caller_id: String,
    pub channel: CommandChannel,
}

/// Semantic payload for starting a fresh conversation turn.
#[derive(Debug, Clone)]
pub struct NewConversationRequest {
    pub caller_id: String,
    pub channel: CommandChannel,
}

/// Runtime capability surface exposed to command side effects.
pub trait SideEffectContext: Send + Sync {
    fn command_context(&self) -> &CommandContext;

    fn start_new_conversation(&self, _request: &NewConversationRequest) -> Result<()> {
        Err(validation_error!(
            "side effect 'start_new_conversation' is not supported for agent_type='{}' agent_id='{}'",
            self.command_context().agent_type,
            self.command_context().agent_id
        ))
    }

    fn send_forward_content(&self, _content: &str) -> Result<()> {
        Err(validation_error!(
            "side effect 'send_forward_content' is not supported for agent_type='{}' agent_id='{}'",
            self.command_context().agent_type,
            self.command_context().agent_id
        ))
    }
}

/// Side effects that a command handler can request.
pub trait CommandSideEffect: Send + Sync {
    fn execute(&self, ctx: &dyn SideEffectContext) -> Result<()>;

    fn name(&self) -> &str {
        "command_side_effect"
    }
}

impl<F> CommandSideEffect for F
where
    F: Fn(&dyn SideEffectContext) -> Result<()> + Send + Sync,
{
    fn execute(&self, ctx: &dyn SideEffectContext) -> Result<()> {
        (self)(ctx)
    }

    fn name(&self) -> &str {
        "command_side_effect_closure"
    }
}

pub type BoxedCommandSideEffect = Box<dyn CommandSideEffect>;

/// Result returned by a command handler.
pub struct CommandResult {
    /// Reply text injected into the LLM conversation (if inject_to_llm is true).
    pub reply: String,
    pub side_effects: Vec<BoxedCommandSideEffect>,
    /// Optional user-visible echo sent as a separate message. For QQ Chat,
    /// this message is persisted via the normal outbound codepath (MySQL
    /// message_record + Redis). For HTTP Stream, it is emitted as a non-bubble
    /// system message.
    pub echo_message: Option<String>,
    /// Whether `reply` should be injected into the LLM conversation history.
    /// Defaults to true. Set to false for commands like /new that clear history.
    pub inject_to_llm: bool,
}

impl CommandResult {
    pub fn add_side_effect<E>(&mut self, effect: E)
    where
        E: CommandSideEffect + 'static,
    {
        self.side_effects.push(Box::new(effect));
    }

    pub fn with_side_effect<E>(mut self, effect: E) -> Self
    where
        E: CommandSideEffect + 'static,
    {
        self.add_side_effect(effect);
        self
    }
}

/// Result of a successful command dispatch, including optional passthrough
/// text for the LLM when the command does not consume all input.
pub struct DispatchResult {
    pub result: CommandResult,
    /// When `accepted_arg_count` is exhausted and text remains, it is preserved
    /// here. The caller should feed this into the LLM as a new conversation
    /// turn after executing the command.
    pub passthrough_text: Option<String>,
}

/// Parsed command preview used by callers that need to inspect a command
/// before deciding whether to execute it.
pub struct CommandPreview<'a> {
    pub definition: &'a CommandDefinition,
    pub args: Vec<String>,
    pub passthrough_text: Option<String>,
}

/// Trait implemented by each command handler.
pub trait CommandHandler: Send + Sync {
    fn handle(&self, ctx: &CommandContext, args: &[String]) -> CommandResult;
}

// PermissionRegistry — stateless permission evaluator.
//
// ## Purpose
//
// PermissionRegistry centralises the logic for evaluating a slice of PermissionRule
// against a CommandContext. It answers: "given these rules and this caller, is the
// caller authorised?"
//
// ## Design
//
// - **No internal state.** The struct is a unit type; the `check` method is a pure
//   function that short-circuits on the first matching `Everyone` or positive match.
// - **Rule evaluation** iterates the rule list linearly. As soon as any rule grants
//   access, `true` is returned. An empty rule list defaults to open access.
// - **Channel-aware matching** is delegated to `CommandContext.caller_id`: QQ users
//   are matched by sender ID, API keys by the `ApiKeys` variant, and custom rules
//   match against `custom_type` + `allow_list`.
// - The design intentionally keeps the evaluator decoupled from storage: permissions
//   are loaded and cached elsewhere, and `check` only sees the resolved rules.

pub struct PermissionRegistry;

impl PermissionRegistry {
    pub fn check(rules: &[PermissionRule], ctx: &CommandContext) -> bool {
        if rules.is_empty() {
            return true;
        }
        for rule in rules {
            match rule {
                PermissionRule::Everyone => return true,
                PermissionRule::QqUsers { allowed_ids } => {
                    if allowed_ids.iter().any(|id| id == &ctx.caller_id) {
                        return true;
                    }
                }
                PermissionRule::ApiKeys { allowed_keys } => {
                    if allowed_keys.iter().any(|k| k == &ctx.caller_id) {
                        return true;
                    }
                }
                PermissionRule::Custom { allow_list, .. } => {
                    if allow_list.iter().any(|id| id == &ctx.caller_id) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

// CommandRegistry — central command router and lifecycle manager.
//
// ## Purpose
//
// CommandRegistry owns the full lifecycle of slash-commands in the system:
// registration, permission management, listing, and dispatch. It is the
// single source of truth for which commands exist and how they map to
// incoming messages.
//
// ## Design
//
// - **Registration** (`register`) accepts a `CommandDefinition` + `Arc<dyn CommandHandler>`.
//   Each command starts with a default `Everyone` permission so it is usable
//   immediately after registration.
// - **Lookup path** (`dispatch`): raw input is tested for a leading `/`, then the
//   command name (case-insensitive) is matched first by primary name and then by
//   aliases. Scope is checked before permissions, so a command that doesn't apply
//   to the current agent type is silently ignored.
// - **Permission enforcement** delegates to `PermissionRegistry::check`. On denial
//   a Chinese-language message is returned; the handler is never invoked.
// - **Mutability model**: permissions are behind a `Mutex` so they can be updated
//   at runtime via `set_permissions` without &mut access to the registry itself.
// - **List APIs** (`list_commands`, `list_permissions`) provide read-only views
//   for admin UIs and the `/help` command.

struct CommandEntry {
    definition: CommandDefinition,
    handler: Arc<dyn CommandHandler>,
    permissions: Mutex<Vec<PermissionRule>>,
}

pub struct CommandRegistry {
    commands: HashMap<String, CommandEntry>,
}

#[derive(Debug, Clone)]
pub struct PermissionCheckResult {
    pub matched: bool,
    pub allowed: bool,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self { commands: HashMap::new() }
    }

    /// Register a command with its handler.
    pub fn register(&mut self, def: CommandDefinition, handler: Arc<dyn CommandHandler>) {
        let name = def.name.clone();
        self.commands.insert(
            name,
            CommandEntry {
                definition: def,
                handler,
                permissions: Mutex::new(vec![PermissionRule::Everyone]),
            },
        );
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
            self.commands
                .values()
                .find(|e| e.definition.aliases.iter().any(|a| a.eq_ignore_ascii_case(&command_name)))
        })?;

        if !entry.definition.scope.matches(&ctx.agent_type, &ctx.agent_id) {
            return None;
        }

        let parsed = parser::parse_command(raw_input, entry.definition.accepted_arg_count)?;
        Some((entry, parsed))
    }

    /// Update permission rules for a registered command.
    pub fn set_permissions(&self, name: &str, rules: Vec<PermissionRule>) {
        if let Some(entry) = self.commands.get(name) {
            if let Ok(mut guard) = entry.permissions.lock() {
                *guard = rules;
            }
        }
    }

    /// List all registered commands (read-only metadata).
    pub fn list_commands(&self) -> Vec<&CommandDefinition> {
        self.commands.values().map(|e| &e.definition).collect()
    }

    /// List all command permissions (for admin API).
    pub fn list_permissions(&self) -> Vec<CommandPermission> {
        self.commands
            .iter()
            .map(|(name, entry)| CommandPermission {
                command_name: name.clone(),
                rules: entry.permissions.lock().unwrap().clone(),
                enabled: true,
            })
            .collect()
    }

    /// Preview a command without executing its handler.
    pub fn preview<'a>(&'a self, ctx: &CommandContext, raw_input: &str) -> Option<CommandPreview<'a>> {
        let (entry, parsed) = self.find_matching_entry(ctx, raw_input)?;
        Some(CommandPreview {
            definition: &entry.definition,
            args: parsed.args,
            passthrough_text: parsed.passthrough_text,
        })
    }

    pub fn check_permission(&self, ctx: &CommandContext, raw_input: &str) -> PermissionCheckResult {
        let Some((entry, _)) = self.find_matching_entry(ctx, raw_input) else {
            return PermissionCheckResult {
                matched: false,
                allowed: false,
            };
        };

        let permissions = entry.permissions.lock().unwrap();
        PermissionCheckResult {
            matched: true,
            allowed: PermissionRegistry::check(&permissions, ctx),
        }
    }

    /// Try to dispatch a raw message as a command. Returns None if the message
    /// does not start with '/', or if the command is not found, or if
    /// permission is denied.
    pub fn dispatch(&self, ctx: &CommandContext, raw_input: &str) -> Option<DispatchResult> {
        let (entry, parsed) = self.find_matching_entry(ctx, raw_input)?;

        // Check permission
        let permissions = entry.permissions.lock().unwrap();
        if !PermissionRegistry::check(&permissions, ctx) {
            return Some(DispatchResult {
                result: CommandResult {
                    reply: "你没有权限使用此命令。".to_string(),
                    side_effects: vec![],
                    echo_message: None,
                    inject_to_llm: false,
                },
                passthrough_text: None,
            });
        }
        drop(permissions);

        let mut result = entry.handler.handle(ctx, &parsed.args);

        if result.reply.is_empty() {
            result.reply = "命令已执行。".to_string();
        }

        Some(DispatchResult {
            result,
            passthrough_text: parsed.passthrough_text,
        })
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    struct TestCommand;

    impl CommandHandler for TestCommand {
        fn handle(&self, _ctx: &CommandContext, args: &[String]) -> CommandResult {
            CommandResult {
                reply: args.join(","),
                side_effects: vec![],
                echo_message: None,
                inject_to_llm: false,
            }
        }
    }

    fn test_context() -> CommandContext {
        CommandContext {
            agent_type: "qq_chat".to_string(),
            agent_id: "agent-1".to_string(),
            caller_id: "caller-1".to_string(),
            channel: CommandChannel::QqChat {
                sender_id: "caller-1".to_string(),
                is_group: false,
                group_id: None,
                target_id: "caller-1".to_string(),
            },
        }
    }

    #[test]
    fn command_definition_defaults_to_no_steer_bypass() {
        let def = CommandDefinition {
            name: "task".to_string(),
            aliases: vec![],
            description: "desc".to_string(),
            scope: CommandScope::All,
            accepted_arg_count: 2,
            allow_steer_bypass: false,
        };
        assert!(!def.allow_steer_bypass);
    }

    #[test]
    fn preview_returns_args_and_passthrough_without_execution() {
        let mut registry = CommandRegistry::new();
        registry.register(
            CommandDefinition {
                name: "task".to_string(),
                aliases: vec![],
                description: "desc".to_string(),
                scope: CommandScope::All,
                accepted_arg_count: 2,
                allow_steer_bypass: true,
            },
            Arc::new(TestCommand),
        );

        let preview = registry
            .preview(&test_context(), "/task cancel abc123 extra words")
            .expect("command preview should exist");
        assert_eq!(preview.definition.name, "task");
        assert!(preview.definition.allow_steer_bypass);
        assert_eq!(preview.args, vec!["cancel", "abc123"]);
        assert_eq!(preview.passthrough_text.as_deref(), Some("extra words"));
    }

    #[test]
    fn dispatch_still_executes_handler_after_preview_refactor() {
        let mut registry = CommandRegistry::new();
        registry.register(
            CommandDefinition {
                name: "task".to_string(),
                aliases: vec!["t".to_string()],
                description: "desc".to_string(),
                scope: CommandScope::All,
                accepted_arg_count: 1,
                allow_steer_bypass: true,
            },
            Arc::new(TestCommand),
        );

        let dispatched = registry
            .dispatch(&test_context(), "/t abc123 trailing text")
            .expect("dispatch should succeed");
        assert_eq!(dispatched.result.reply, "abc123");
        assert_eq!(dispatched.passthrough_text.as_deref(), Some("trailing text"));
    }
}
