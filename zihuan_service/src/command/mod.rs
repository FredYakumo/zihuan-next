mod help_command;
mod new_command;
mod task_command;

use std::sync::{Arc, OnceLock};

use zihuan_core::command::{CommandDefinition, CommandRegistry, CommandScope};
use zihuan_core::task_context::AgentTaskRuntime;

use help_command::HelpCommand;
use new_command::NewCommand;
use task_command::TaskCommand;

static GLOBAL_COMMAND_REGISTRY: OnceLock<Arc<CommandRegistry>> = OnceLock::new();
static GLOBAL_TASK_RUNTIME: OnceLock<Arc<dyn AgentTaskRuntime>> = OnceLock::new();

/// Initialize the global command registry. Must be called once during startup.
pub fn init_global_command_registry() -> Arc<CommandRegistry> {
    let registry = build_command_registry();
    GLOBAL_COMMAND_REGISTRY
        .set(Arc::clone(&registry))
        .ok()
        .expect("command registry already initialized");
    registry
}

/// Set the global task runtime reference. Called during agent startup.
pub fn set_global_task_runtime(runtime: Arc<dyn AgentTaskRuntime>) {
    GLOBAL_TASK_RUNTIME
        .set(runtime)
        .ok()
        .expect("task runtime already initialized");
}

/// Get a reference to the global task runtime.
pub fn global_task_runtime() -> Option<Arc<dyn AgentTaskRuntime>> {
    GLOBAL_TASK_RUNTIME.get().cloned()
}

/// Get a reference to the global command registry.
pub fn global_command_registry() -> Option<Arc<CommandRegistry>> {
    GLOBAL_COMMAND_REGISTRY.get().cloned()
}

/// Build a human-readable help text from the global command registry.
///
/// Returns `None` if the registry hasn't been initialized yet. Used by
/// `get_function_list` tool and `AskToolList` intent shortcut so they
/// always reflect the live set of registered commands.
pub fn build_help_text() -> Option<String> {
    let registry = global_command_registry()?;
    let commands = registry.list_commands();

    let mut lines: Vec<String> = commands
        .iter()
        .map(|def| {
            let aliases_str = if def.aliases.is_empty() {
                String::new()
            } else {
                format!(" (别名: {})", def.aliases.join(", "))
            };
            format!("/{} — {}{}", def.name, def.description, aliases_str)
        })
        .collect();

    if lines.is_empty() {
        lines.push("暂无可用命令。".to_string());
    }

    Some(lines.join("\n"))
}

/// build_command_registry — assembles the default command set.
//
// ## Purpose
//
// Creates and populates the global `CommandRegistry` with all built-in
// slash-commands. Called once during service startup by
// `init_global_command_registry`.
//
// ## Design
//
// - Registers `NewCommand` under `/new` (aliases: `clear`, `reset`) and
//   `TaskCommand` under `/task`.
// - The `/help` command uses a **lazy registry reference** (`Arc<Mutex<Option<...>>>`)
//   to avoid a circular dependency: the help handler needs the registry, but the
//   registry creation calls this builder. The reference is resolved after the
//   registry is fully constructed.
// - All built-in commands are scoped to `CommandScope::All` (available in both
//   QQ Chat and HTTP Stream agents).
pub fn build_command_registry() -> Arc<CommandRegistry> {
    let mut registry = CommandRegistry::new();

    let reg_ptr = Arc::new(std::sync::Mutex::new(None::<Arc<CommandRegistry>>));

    registry.register(
        CommandDefinition {
            name: "new".to_string(),
            aliases: vec!["clear".to_string(), "reset".to_string()],
            description: "清除对话历史，开始新对话".to_string(),
            scope: CommandScope::All,
            accepted_arg_count: 0,
        },
        Arc::new(NewCommand),
    );

    registry.register(
        CommandDefinition {
            name: "task".to_string(),
            aliases: vec![],
            description: "查看最近任务状态".to_string(),
            scope: CommandScope::All,
            accepted_arg_count: 2,
        },
        Arc::new(TaskCommand),
    );

    registry.register(
        CommandDefinition {
            name: "help".to_string(),
            aliases: vec!["h".to_string()],
            description: "列出可用命令".to_string(),
            scope: CommandScope::All,
            accepted_arg_count: 0,
        },
        Arc::new(HelpCommand {
            registry: reg_ptr.clone(),
        }),
    );

    let registry = Arc::new(registry);
    *reg_ptr.lock().unwrap() = Some(Arc::clone(&registry));

    registry
}
