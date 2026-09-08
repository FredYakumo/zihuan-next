use std::sync::Arc;

use crate::command::effect::Effect;
use crate::error::Result;
use crate::task_context::AgentTaskRuntime;

/// Pure built-in command logic shared by every channel runtime (`builtin://*`).
///
/// These functions only produce serializable [`Effect`]s; they never touch a
/// channel. The caller (QQ / Dashboard runtime) decides how to render effects.
/// `owner_id` is the ownership key for task scoping (the command caller).
///
/// Rendering contract per effect (kept aligned with the pre-engine behavior):
/// - `Effect::Text` — QQ renders a direct reply bubble; Dashboard an assistant
///   message.
/// - `Effect::Forward` — QQ renders a forward node (used for task detail which
///   QQ historically forwarded); Dashboard renders assistant text.
pub fn execute_builtin(
    op: &str,
    args: &[String],
    owner_id: &str,
) -> Option<Result<Vec<Effect>>> {
    match op {
        "builtin://new" => Some(Ok(vec![
            Effect::StartNewConversation,
            // Old QQ /new echoed this text as its visible bubble; Dashboard
            // rendered no assistant message for /new (new session only).
            Effect::Notice("已清空之前对话内容".to_string()),
        ])),
        "builtin://help" => Some(Ok(vec![Effect::Text(help_text())])),
        "builtin://task" => Some(dispatch_task(args, owner_id)),
        "builtin://task/latest" => Some(task_latest(owner_id)),
        "builtin://task/list" => Some(task_list(owner_id)),
        "builtin://task/detail" => {
            Some(task_detail(args.first().map(String::as_str).unwrap_or(""), owner_id))
        }
        "builtin://task/cancel" => {
            Some(task_cancel(args.first().map(String::as_str).unwrap_or(""), owner_id))
        }
        _ => None,
    }
}

fn dispatch_task(args: &[String], owner_id: &str) -> Result<Vec<Effect>> {
    match args.first().map(String::as_str) {
        None => task_latest(owner_id),
        Some("list") => task_list(owner_id),
        Some("cancel") => {
            let task_id = args.get(1).map(String::as_str).unwrap_or("");
            task_cancel(task_id, owner_id)
        }
        Some(task_id) => task_detail(task_id, owner_id),
    }
}

fn help_text() -> String {
    super::build_help_text().unwrap_or_else(|| "命令注册表尚未初始化。".to_string())
}

fn task_runtime() -> Option<Arc<dyn AgentTaskRuntime>> {
    crate::command::global_task_runtime()
}

fn status_label(status: crate::task_context::AgentTaskStatus) -> &'static str {
    match status {
        crate::task_context::AgentTaskStatus::Running => "进行中",
        crate::task_context::AgentTaskStatus::WaitingAuth => "等待授权",
        crate::task_context::AgentTaskStatus::Success => "已完成",
        crate::task_context::AgentTaskStatus::Failed => "失败",
        crate::task_context::AgentTaskStatus::Stopped => "已停止",
    }
}

fn render_task_detail(task: &crate::task_context::AgentTaskInfo) -> String {
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
    if let Some(error_message) =
        task.error_message.as_deref().filter(|value| !value.trim().is_empty())
    {
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

fn task_latest(owner_id: &str) -> Result<Vec<Effect>> {
    let Some(runtime) = task_runtime() else {
        return Ok(vec![Effect::Text("暂无任务".to_string())]);
    };
    let runtime: &dyn AgentTaskRuntime = &*runtime;
    let mut tasks = runtime.list_tasks(owner_id);
    tasks.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    match tasks.into_iter().next() {
        Some(task) => Ok(vec![Effect::Forward(render_task_detail(&task))]),
        None => Ok(vec![Effect::Text("你当前没有后台任务。".to_string())]),
    }
}

fn task_list(owner_id: &str) -> Result<Vec<Effect>> {
    let Some(runtime) = task_runtime() else {
        return Ok(vec![Effect::Text("暂无任务".to_string())]);
    };
    let runtime: &dyn AgentTaskRuntime = &*runtime;
    let mut tasks = runtime.list_tasks(owner_id);
    tasks.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    if tasks.is_empty() {
        return Ok(vec![Effect::Text("你当前没有进行中的后台任务。".to_string())]);
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
    lines.push(
        "使用 /task 查看最近任务，/task <id> 查看指定任务，/task cancel <id> 取消任务。"
            .to_string(),
    );
    // Old QQ rendered the list as a direct text echo (not a forward).
    Ok(vec![Effect::Text(lines.join("\n"))])
}

fn task_detail(task_id: &str, owner_id: &str) -> Result<Vec<Effect>> {
    let Some(runtime) = task_runtime() else {
        return Ok(vec![Effect::Text("暂无任务".to_string())]);
    };
    let runtime: &dyn AgentTaskRuntime = &*runtime;
    match runtime.query_owned_task(task_id, owner_id) {
        Some(task) => Ok(vec![Effect::Forward(render_task_detail(&task))]),
        None => Ok(vec![Effect::Text(format!(
            "未找到任务 '{}'。使用 /task list 查看你的任务列表。",
            task_id
        ))]),
    }
}

fn task_cancel(task_id: &str, owner_id: &str) -> Result<Vec<Effect>> {
    let Some(runtime) = task_runtime() else {
        return Ok(vec![Effect::Text("暂无任务".to_string())]);
    };
    let runtime: &dyn AgentTaskRuntime = &*runtime;
    if task_id.is_empty() {
        return Ok(vec![Effect::Text(
            "用法: /task cancel <任务ID>\n使用 /task list 查看你的任务列表。".to_string(),
        )]);
    }
    match runtime.query_owned_task(task_id, owner_id) {
        Some(task) => {
            if task.status != crate::task_context::AgentTaskStatus::Running {
                return Ok(vec![Effect::Text(format!(
                    "任务 '{}' 当前状态为 {}，无法取消。",
                    task.task_name,
                    status_label(task.status)
                ))]);
            }
            if runtime.cancel_task(task_id) {
                Ok(vec![Effect::Text(format!(
                    "已发送取消请求，任务 '{}' 将停止。",
                    task.task_name
                ))])
            } else {
                Ok(vec![Effect::Text(format!(
                    "取消任务 '{}' 失败，任务可能已结束。",
                    task.task_name
                ))])
            }
        }
        None => Ok(vec![Effect::Text(format!(
            "未找到任务 '{}'。使用 /task list 查看你的任务列表。",
            task_id
        ))]),
    }
}
