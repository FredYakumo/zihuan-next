use zihuan_core::command::{CommandChannel, CommandContext, CommandHandler, CommandResult};

/// TaskCommand — `/task` command for querying and cancelling background tasks.
///
/// ## Purpose
///
/// Lets users inspect and cancel their background tasks (created by async
/// tool calls in the Brain loop). Supports four forms:
///
/// - `/task` — shows detailed status of the caller's most recent task.
/// - `/task list` — lists all tasks owned by the caller.
/// - `/task <task_id>` — shows detailed status (progress + result) for one task.
/// - `/task cancel <task_id>` — requests cancellation of a running task.
///
/// ## Design
///
/// - Queries the global [`AgentTaskRuntime`] set during agent startup.
/// - Only returns/cancels tasks whose `owner_id` matches the caller.
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
        lines.push(format!("完成时间: {}", finished_at.format("%Y-%m-%d %H:%M:%S")));
    }
    if let Some(summary) = task.result_summary.as_deref().filter(|value| !value.trim().is_empty()) {
        lines.push(String::new());
        lines.push("结果:".to_string());
        lines.push(summary.to_string());
    }
    if let Some(error_message) = task.error_message.as_deref().filter(|value| !value.trim().is_empty()) {
        lines.push(String::new());
        lines.push("错误:".to_string());
        lines.push(error_message.to_string());
    }
    if !task.progress.is_empty() {
        lines.push(String::new());
        lines.push(format!("进展 ({}):", task.progress.len()));
        for (index, item) in task.progress.iter().enumerate() {
            lines.push(format!("{}. {}", index + 1, item));
        }
    }

    lines.join("\n")
}

fn simple_result(reply: String, echo: Option<String>) -> CommandResult {
    let echo_message = echo.unwrap_or_else(|| reply.clone());
    CommandResult {
        reply,
        side_effects: vec![],
        echo_message: Some(echo_message),
        inject_to_llm: false,
    }
}

impl CommandHandler for TaskCommand {
    fn handle(&self, ctx: &CommandContext, args: &[String]) -> CommandResult {
        let runtime = match crate::command::global_task_runtime() {
            Some(rt) => rt,
            None => return simple_result("暂无任务".to_string(), None),
        };

        let runtime: &dyn zihuan_core::task_context::AgentTaskRuntime = &*runtime;

        if args.is_empty() {
            return show_latest_task(runtime, ctx);
        }

        if args[0] == "list" {
            return list_tasks(runtime, ctx);
        }

        if args[0] == "cancel" {
            let task_id = args.get(1).map(|s| s.as_str()).unwrap_or("");
            if task_id.is_empty() {
                return simple_result(
                    "用法: /task cancel <任务ID>\n使用 /task list 查看你的任务列表。".to_string(),
                    None,
                );
            }
            return cancel_task(runtime, ctx, task_id);
        }

        let task_id = args.first().map(String::as_str).unwrap_or("");
        if task_id.is_empty() {
            return simple_result("用法: /task <任务ID>".to_string(), None);
        }

        show_task_detail(runtime, ctx, task_id)
    }
}

fn list_tasks(runtime: &dyn zihuan_core::task_context::AgentTaskRuntime, ctx: &CommandContext) -> CommandResult {
    let mut tasks = runtime.list_tasks(&ctx.caller_id);
    tasks.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    if tasks.is_empty() {
        return simple_result("你当前没有进行中的后台任务。".to_string(), None);
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
    lines.push("使用 /task 查看最近任务，/task <id> 查看指定任务，/task cancel <id> 取消任务。".to_string());
    let reply = lines.join("\n");
    simple_result(reply, None)
}

fn show_task_detail(
    runtime: &dyn zihuan_core::task_context::AgentTaskRuntime,
    ctx: &CommandContext,
    task_id: &str,
) -> CommandResult {
    match runtime.query_owned_task(task_id, &ctx.caller_id) {
        Some(task) => {
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
                simple_result(detail.clone(), Some(detail))
            }
        }
        None => {
            let reply = format!("未找到任务 '{}'。使用 /task list 查看你的任务列表。", task_id);
            simple_result(reply, None)
        }
    }
}

fn show_latest_task(runtime: &dyn zihuan_core::task_context::AgentTaskRuntime, ctx: &CommandContext) -> CommandResult {
    let mut tasks = runtime.list_tasks(&ctx.caller_id);
    tasks.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    match tasks.into_iter().next() {
        Some(task) => {
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
                simple_result(detail.clone(), Some(detail))
            }
        }
        None => simple_result("你当前没有后台任务。".to_string(), None),
    }
}

fn cancel_task(
    runtime: &dyn zihuan_core::task_context::AgentTaskRuntime,
    ctx: &CommandContext,
    task_id: &str,
) -> CommandResult {
    match runtime.query_owned_task(task_id, &ctx.caller_id) {
        Some(task) => {
            if task.status != zihuan_core::task_context::AgentTaskStatus::Running {
                return simple_result(
                    format!("任务 '{}' 当前状态为 {}，无法取消。", task.task_name, status_label(task.status)),
                    None,
                );
            }
            if runtime.cancel_task(task_id) {
                simple_result(format!("已发送取消请求，任务 '{}' 将停止。", task.task_name), None)
            } else {
                simple_result(format!("取消任务 '{}' 失败，任务可能已结束。", task.task_name), None)
            }
        }
        None => {
            let reply = format!("未找到任务 '{}'。使用 /task list 查看你的任务列表。", task_id);
            simple_result(reply, None)
        }
    }
}
