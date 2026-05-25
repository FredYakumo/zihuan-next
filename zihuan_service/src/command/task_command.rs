use zihuan_core::command::{CommandContext, CommandHandler, CommandResult};

/// TaskCommand — `/task` placeholder.
///
/// ## Purpose
///
/// Reserves the `/task` namespace for future background-task inspection. Currently
/// returns a polite "not yet enabled" message so users aren't confused by a
/// "command not found" response.
///
/// ## Design
///
/// - Accepts optional task-name arguments; if none given, a generic message is shown.
/// - No side effects are produced. Once the background-task system is implemented,
///   this handler will be replaced with actual task-query logic.

pub struct TaskCommand;

impl CommandHandler for TaskCommand {
    fn handle(&self, _ctx: &CommandContext, args: &[String]) -> CommandResult {
        if args.is_empty() {
            CommandResult {
                reply: "任务系统尚未启用。完成后台任务改造后将支持查看任务状态。".to_string(),
                side_effects: vec![],
                echo_message: None,
                inject_to_llm: true,
            }
        } else {
            CommandResult {
                reply: format!(
                    "任务 '{}' 系统尚未启用。完成后台任务改造后将支持查看任务详情。",
                    args.join(" ")
                ),
                side_effects: vec![],
                echo_message: None,
                inject_to_llm: true,
            }
        }
    }
}
