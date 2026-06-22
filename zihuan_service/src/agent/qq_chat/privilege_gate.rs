use std::sync::Arc;

use zihuan_core::command::{CommandContext, CommandRegistry};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::Result;

use crate::agent::qq_chat::privilege_store::{
    create_privilege_auth, has_active_privilege_blocking, verify_privilege_auth, PrivilegeAuthStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QqPrivilegedCommand {
    LearnGlobalStyle,
    LearnGroupStyle,
}

impl QqPrivilegedCommand {
    pub fn command_name(self) -> &'static str {
        match self {
            Self::LearnGlobalStyle => "learn_global_style",
            Self::LearnGroupStyle => "learn_group_style",
        }
    }
}

pub enum PrivilegeGateOutcome {
    Authorized,
    Denied(String),
}

#[derive(Debug, Clone)]
pub struct PendingAuthorizedCommand {
    pub command: QqPrivilegedCommand,
    pub pending_task_id: Option<String>,
    pub pending_target_id: Option<String>,
    pub pending_group_id: Option<i64>,
    pub pending_is_group: bool,
}

pub enum AuthCommandOutcome {
    Reply(String),
    Resume {
        message: String,
        pending: PendingAuthorizedCommand,
    },
}

pub fn parse_privileged_command(raw_input: &str) -> Option<(String, Vec<String>)> {
    let trimmed = raw_input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed[1..].split_whitespace();
    let name = parts.next()?.to_string();
    match name.as_str() {
        "auth" | "learn_global_style" | "learn_group_style" => Some((name, parts.map(ToOwned::to_owned).collect())),
        _ => None,
    }
}

pub fn render_privilege_auth_prompt(command_name: &str) -> String {
    let label = match command_name {
        "learn_global_style" => "学习全局语言风格",
        "learn_group_style" => "学习群聊语言风格",
        _ => command_name,
    };
    format!("「{label}」需要授权确认。\n请在 5 分钟内输入 /auth <密钥> 完成授权。")
}

pub fn render_auth_usage_prompt() -> String {
    "用法: /auth <密钥>".to_string()
}

pub fn handle_auth_command(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    auth_key: &str,
) -> Result<AuthCommandOutcome> {
    let auth_key = auth_key.trim();
    if auth_key.is_empty() {
        return Ok(AuthCommandOutcome::Reply(render_auth_usage_prompt()));
    }

    let status = run_blocking_future(verify_privilege_auth(connection, agent_id, sender_id, auth_key))?;
    Ok(match status {
        PrivilegeAuthStatus::Elevated { until, record } => match purpose_to_privileged_command(&record.purpose) {
            Some(command) => AuthCommandOutcome::Resume {
                message: format!("授权成功。正在自动继续执行 `/{} `。", command.command_name())
                    .trim()
                    .to_string(),
                pending: PendingAuthorizedCommand {
                    command,
                    pending_task_id: record.pending_task_id,
                    pending_target_id: record.pending_target_id,
                    pending_group_id: record.pending_group_id,
                    pending_is_group: record.pending_is_group,
                },
            },
            None => AuthCommandOutcome::Reply(format!("授权成功。你已进入特权模式，有效期至 {until}。")),
        },
        PrivilegeAuthStatus::NotFound => {
            AuthCommandOutcome::Reply("当前没有待验证的授权密钥，请重新触发需要特权的命令。".to_string())
        }
        PrivilegeAuthStatus::Pending(_) => AuthCommandOutcome::Reply("当前密钥仍待验证，请重新输入。".to_string()),
        PrivilegeAuthStatus::Failed(message) => AuthCommandOutcome::Reply(message),
    })
}

pub fn enqueue_pending_privileged_command(
    registry: &Arc<CommandRegistry>,
    cmd_ctx: &CommandContext,
    connection: &RelationalDbConnection,
    privileged_command: QqPrivilegedCommand,
    pending_task_id: Option<&str>,
) -> Result<PrivilegeGateOutcome> {
    let raw_command = format!("/{}", privileged_command.command_name());
    let permission_check = registry.check_permission(cmd_ctx, &raw_command);
    if !permission_check.matched || !permission_check.allowed {
        return Ok(PrivilegeGateOutcome::Denied("你没有权限使用此命令。".to_string()));
    }

    if has_active_privilege_blocking(connection, &cmd_ctx.agent_id, &cmd_ctx.caller_id)? {
        return Ok(PrivilegeGateOutcome::Authorized);
    }

    let record = run_blocking_future(create_privilege_auth(
        connection,
        &cmd_ctx.agent_id,
        &cmd_ctx.caller_id,
        privileged_command.command_name(),
        pending_task_id,
        match &cmd_ctx.channel {
            zihuan_core::command::CommandChannel::QqChat { target_id, .. } => Some(target_id.as_str()),
            _ => None,
        },
        match &cmd_ctx.channel {
            zihuan_core::command::CommandChannel::QqChat { group_id, .. } => *group_id,
            _ => None,
        },
        matches!(
            cmd_ctx.channel,
            zihuan_core::command::CommandChannel::QqChat { is_group: true, .. }
        ),
    ))?;
    Ok(PrivilegeGateOutcome::Denied(render_privilege_auth_prompt(
        privileged_command.command_name(),
    )))
}

fn run_blocking_future<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        tokio::runtime::Runtime::new()?.block_on(future)
    }
}

fn purpose_to_privileged_command(purpose: &str) -> Option<QqPrivilegedCommand> {
    match purpose {
        "learn_global_style" => Some(QqPrivilegedCommand::LearnGlobalStyle),
        "learn_group_style" => Some(QqPrivilegedCommand::LearnGroupStyle),
        _ => None,
    }
}
