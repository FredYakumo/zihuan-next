use std::sync::{Arc, Mutex};

use zihuan_core::command::{CommandContext, CommandHandler, CommandRegistry, CommandResult};

/// HelpCommand — `/help`, `/h` handler.
///
/// ## Purpose
///
/// Lists all registered slash-commands with their descriptions and aliases.
///

pub struct HelpCommand {
    pub registry: Arc<Mutex<Option<Arc<CommandRegistry>>>>,
}

impl CommandHandler for HelpCommand {
    fn handle(&self, _ctx: &CommandContext, _args: &[String]) -> CommandResult {
        let guard = self.registry.lock().unwrap();
        let Some(ref reg) = *guard else {
            return CommandResult {
                reply: "命令注册表尚未初始化。".to_string(),
                side_effects: vec![],
                echo_message: None,
                inject_to_llm: true,
            };
        };
        let commands = reg.list_commands();

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

        CommandResult {
            reply: lines.join("\n"),
            side_effects: vec![],
            echo_message: None,
            inject_to_llm: true,
        }
    }
}
