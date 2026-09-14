use std::sync::Arc;

use zihuan_core::command::{CommandRegistry, CommandScope, CommandSpec, Step};
use zihuan_core::task_context::AgentTaskRuntime;

/// Initialize the global command registry. Must be called once during startup.
pub fn init_global_command_registry() -> Arc<CommandRegistry> {
    let registry = build_command_registry();
    zihuan_core::command::set_global_command_registry(Arc::clone(&registry))
        .expect("command registry already initialized");
    registry
}

/// Set the global task runtime reference. Called during agent startup.
///
/// Replaces any previously set runtime — each agent start updates the global
/// runtime so that slash commands target the currently active agent.
pub fn set_global_task_runtime(runtime: Arc<dyn AgentTaskRuntime>) {
    zihuan_core::command::set_global_task_runtime(runtime);
}

/// Get a reference to the global task runtime.
pub fn global_task_runtime() -> Option<Arc<dyn AgentTaskRuntime>> {
    zihuan_core::command::global_task_runtime()
}

/// Get a reference to the global command registry.
pub fn global_command_registry() -> Option<Arc<CommandRegistry>> {
    zihuan_core::command::global_command_registry()
}

/// Build a human-readable help text from the global command registry.
///
/// Returns `None` if the registry hasn't been initialized yet. Used by
/// `get_function_list` tool and `AskToolList` intent shortcut so they
/// always reflect the live set of registered commands.
pub fn build_help_text() -> Option<String> {
    zihuan_core::command::build_help_text()
}

/// build_command_registry — assembles the default command set.
//
// ## Purpose
//
// Creates and populates the global `CommandRegistry` with all slash-commands
// as data-driven `CommandSpec`s. Called once during service startup by
// `init_global_command_registry`.
//
// ## Design
//
// - Builtin commands (`new`, `task`, `help`) execute through `builtin://*` step
//   ops shared by every channel runtime.
// - QQ privileged specs (`auth`, `emotion`, `adjust_emotion`,
//   `learn_global_style`, `learn_group_style`) declare only their data; the QQ
//   channel runtime implements the `ims://*` ops. They carry `allow_steer_bypass:
//   false` so they queue behind a busy session instead of executing out of band.
// - All builtin commands are `CommandScope::All`.
pub fn build_command_registry() -> Arc<CommandRegistry> {
    let mut registry = CommandRegistry::new();

    registry.register(CommandSpec {
        name: "new".to_string(),
        aliases: vec!["clear".to_string(), "reset".to_string()],
        description: "清除对话历史，开始新对话".to_string(),
        scope: CommandScope::All,
        accepted_arg_count: 0,
        allow_steer_bypass: false,
        setup: Vec::new(),
        conditions: Vec::new(),
        body: vec![Step::new("builtin://new")],
    });

    registry.register(CommandSpec {
        name: "task".to_string(),
        aliases: Vec::new(),
        description: "查看最近任务状态".to_string(),
        scope: CommandScope::All,
        accepted_arg_count: 2,
        allow_steer_bypass: true,
        setup: Vec::new(),
        conditions: Vec::new(),
        body: vec![Step::new("builtin://task")],
    });

    registry.register(CommandSpec {
        name: "help".to_string(),
        aliases: vec!["h".to_string()],
        description: "列出可用命令".to_string(),
        scope: CommandScope::All,
        accepted_arg_count: 0,
        allow_steer_bypass: false,
        setup: Vec::new(),
        conditions: Vec::new(),
        body: vec![Step::new("builtin://help")],
    });

    registry.register(CommandSpec {
        name: "auth".to_string(),
        aliases: Vec::new(),
        description: "输入临时授权密钥，完成特权提权".to_string(),
        scope: CommandScope::QqChat,
        accepted_arg_count: 1,
        allow_steer_bypass: false,
        setup: Vec::new(),
        conditions: Vec::new(),
        body: vec![Step::new("ims://auth/verify")],
    });

    registry.register(CommandSpec {
        name: "learn_global_style".to_string(),
        aliases: Vec::new(),
        description: "学习全局聊天语言风格（需管理员权限和特权）".to_string(),
        scope: CommandScope::QqChat,
        accepted_arg_count: 0,
        allow_steer_bypass: false,
        setup: vec![Step::with_params(
            "ims://style/prepare_waiting_task",
            "创建等待授权的全局风格学习任务",
            serde_json::json!({ "display": "学习全局语言风格" }),
        )],
        conditions: vec![Step::with_params(
            "ims://privilege/active",
            "校验管理员特权",
            serde_json::json!({ "purpose": "learn_global_style" }),
        )],
        body: vec![Step::with_params(
            "ims://style/start",
            "启动全局风格学习",
            serde_json::json!({ "scope": "global" }),
        )],
    });

    registry.register(CommandSpec {
        name: "learn_group_style".to_string(),
        aliases: Vec::new(),
        description: "学习当前群聊语言风格（需管理员权限和特权）".to_string(),
        scope: CommandScope::QqChat,
        accepted_arg_count: 0,
        allow_steer_bypass: false,
        setup: vec![Step::with_params(
            "ims://style/prepare_waiting_task",
            "创建等待授权的群聊风格学习任务",
            serde_json::json!({ "display": "学习群聊语言风格" }),
        )],
        conditions: vec![Step::with_params(
            "ims://privilege/active",
            "校验管理员特权",
            serde_json::json!({ "purpose": "learn_group_style" }),
        )],
        body: vec![Step::with_params(
            "ims://style/start",
            "启动群聊风格学习",
            serde_json::json!({ "scope": "group" }),
        )],
    });

    registry.register(CommandSpec {
        name: "emotion".to_string(),
        aliases: Vec::new(),
        description: "查看当前 Agent 情绪维度（需管理员权限和特权）".to_string(),
        scope: CommandScope::QqChat,
        accepted_arg_count: 0,
        allow_steer_bypass: false,
        setup: Vec::new(),
        conditions: vec![Step::with_params(
            "ims://privilege/active",
            "校验管理员特权",
            serde_json::json!({ "purpose": "emotion" }),
        )],
        body: vec![Step::with_params(
            "ims://emotion/execute",
            "读取当前情绪维度",
            serde_json::json!({ "command": "emotion" }),
        )],
    });

    registry.register(CommandSpec {
        name: "adjust_emotion".to_string(),
        aliases: Vec::new(),
        description: "调整当前 Agent 情绪维度（需管理员权限和特权）".to_string(),
        scope: CommandScope::QqChat,
        accepted_arg_count: 2,
        allow_steer_bypass: false,
        setup: Vec::new(),
        conditions: vec![Step::with_params(
            "ims://privilege/active",
            "校验管理员特权",
            serde_json::json!({ "purpose": "adjust_emotion" }),
        )],
        body: vec![Step::with_params(
            "ims://emotion/execute",
            "调整情绪维度",
            serde_json::json!({ "command": "adjust_emotion" }),
        )],
    });

    Arc::new(registry)
}
