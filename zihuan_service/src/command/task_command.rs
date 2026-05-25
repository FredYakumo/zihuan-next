use zihuan_core::command::{CommandChannel, CommandContext, CommandHandler, CommandResult};

/// TaskCommand — `/task` command for querying background task status.
///
/// ## Purpose
///
/// Lets users inspect their background tasks (created by async tool calls
/// in the Brain loop). Supports two forms:
///
/// - `/task` — lists all tasks owned by the caller.
/// - `/task <task_id>` — shows detailed status (progress + result) for one task.
///
/// ## Design
///
/// - Queries the global [`AgentTaskRuntime`] set during agent startup.
/// - Only returns tasks whose `owner_id` matches the caller.
/// - When no task runtime is available, returns a brief "not available" message.

pub struct TaskCommand;

fn status_label(status: zihuan_core::task_context::AgentTaskStatus) -> &'static str {
    match status {
        zihuan_core::task_context::AgentTaskStatus::Running => "进行中",
        zihuan_core::task_context::AgentTaskStatus::Success => "已完成",
        zihuan_core::task_context::AgentTaskStatus::Failed => "失败",
        zihuan_core::task_context::AgentTaskStatus::Stopped => "已停止",
    }
}

fn render_task_detail(task: &zihuan_core::task_context::AgentTaskInfo) -> String {
    let mut lines = vec![
        format!("任务: {}", task.task_name),
        format!("ID: {}", task.task_id),
        format!("状态: {}", status_label(task.status)),
        format!("创建时间: {}", task.created_at.format("%Y-%m-%d %H:%M:%S")),
    ];

    if let Some(finished_at) = task.finished_at {
        lines.push(format!(
            "完成时间: {}",
            finished_at.format("%Y-%m-%d %H:%M:%S")
        ));
    }
    if let Some(summary) = task.result_summary.as_deref().filter(|value| !value.trim().is_empty()) {
        lines.push(String::new());
        lines.push("结果:".to_string());
        lines.push(summary.to_string());
    }
    if let Some(error_message) = task
        .error_message
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(String::new());
        lines.push("错误:".to_string());
        lines.push(error_message.to_string());
    }
    if !task.progress.is_empty() {
        lines.push(String::new());
        lines.push("进展:".to_string());
        for item in &task.progress {
            lines.push(format!("- {item}"));
        }
    }

    lines.join("\n")
}

impl CommandHandler for TaskCommand {
    fn handle(&self, ctx: &CommandContext, args: &[String]) -> CommandResult {
        let runtime = match crate::command::global_task_runtime() {
            Some(rt) => rt,
            None => {
                let reply = "暂无任务".to_string();
                return CommandResult {
                    reply: reply.clone(),
                    side_effects: vec![],
                    echo_message: Some(reply),
                    inject_to_llm: false,
                };
            }
        };

        if args.is_empty() {
            let mut tasks = runtime.list_tasks(&ctx.caller_id);
            tasks.sort_by(|left, right| right.created_at.cmp(&left.created_at));
            if tasks.is_empty() {
                let reply = "你当前没有进行中的后台任务。".to_string();
                return CommandResult {
                    reply: reply.clone(),
                    side_effects: vec![],
                    echo_message: Some(reply),
                    inject_to_llm: false,
                };
            }

            let mut lines: Vec<String> = vec!["你的后台任务：".to_string()];
            for task in &tasks {
                lines.push(format!(
                    "  [{}] {} — {}",
                    &task.task_id[..task.task_id.len().min(8)],
                    task.task_name,
                    status_label(task.status)
                ));
            }
            lines.push("使用 /task <id> 查看任务详情。".to_string());
            let reply = lines.join("\n");

            CommandResult {
                reply: reply.clone(),
                side_effects: vec![],
                echo_message: Some(reply),
                inject_to_llm: false,
            }
        } else {
            let task_id = args.join(" ");
            match runtime.query_task(&task_id) {
                Some(task) => {
                    if task.owner_id.as_deref() != Some(&ctx.caller_id) {
                        let reply = "你没有权限查看此任务，或任务不存在。".to_string();
                        return CommandResult {
                            reply: reply.clone(),
                            side_effects: vec![],
                            echo_message: Some(reply),
                            inject_to_llm: false,
                        };
                    }

                    let detail = render_task_detail(&task);
                    if matches!(ctx.channel, CommandChannel::QqChat { .. }) {
                        let mut result = CommandResult {
                            reply: "已发送任务详情。".to_string(),
                            side_effects: vec![],
                            echo_message: None,
                            inject_to_llm: false,
                        };
                        result.add_side_effect(move |effect_ctx: &dyn zihuan_core::command::SideEffectContext| {
                            effect_ctx.send_forward_content(&detail)
                        });
                        result
                    } else {
                        CommandResult {
                            reply: detail.clone(),
                            side_effects: vec![],
                            echo_message: Some(detail),
                            inject_to_llm: false,
                        }
                    }
                }
                None => {
                    let reply = format!("未找到任务 '{}'。使用 /task 查看你的任务列表。", task_id);
                    CommandResult {
                        reply: reply.clone(),
                        side_effects: vec![],
                        echo_message: Some(reply),
                        inject_to_llm: false,
                    }
                }
            }
        }
    }
}
