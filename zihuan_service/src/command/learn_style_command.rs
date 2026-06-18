use zihuan_core::command::{CommandContext, CommandHandler, CommandResult};

pub struct LearnStyleCommand;

impl CommandHandler for LearnStyleCommand {
    fn handle(&self, _ctx: &CommandContext, _args: &[String]) -> CommandResult {
        CommandResult {
            reply: "该命令仅能在 QQ Chat Agent 运行时中使用。".to_string(),
            side_effects: vec![],
            echo_message: None,
            inject_to_llm: false,
        }
    }
}
