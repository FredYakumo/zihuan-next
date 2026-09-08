use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::Result;

use crate::qq_chat::privilege_store::{verify_privilege_auth, PrivilegeAuthStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QqPrivilegedCommand {
    LearnGlobalStyle,
    LearnGroupStyle,
    Emotion,
    AdjustEmotion,
}

impl QqPrivilegedCommand {
    pub fn command_name(self) -> &'static str {
        match self {
            Self::LearnGlobalStyle => "learn_global_style",
            Self::LearnGroupStyle => "learn_group_style",
            Self::Emotion => "emotion",
            Self::AdjustEmotion => "adjust_emotion",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PendingAuthorizedCommand {
    pub command: QqPrivilegedCommand,
    pub pending_task_id: Option<String>,
    pub pending_target_id: Option<String>,
    pub pending_group_id: Option<i64>,
    pub pending_is_group: bool,
    pub pending_args: Vec<String>,
}

pub enum AuthCommandOutcome {
    Reply(String),
    Resume {
        message: String,
        pending: PendingAuthorizedCommand,
    },
}

pub fn render_privilege_auth_prompt(command_name: &str) -> String {
    let label = match command_name {
        "learn_global_style" => "学习全局语言风格",
        "learn_group_style" => "学习群聊语言风格",
        "emotion" => "查看当前 Agent 情绪维度",
        "adjust_emotion" => "调整当前 Agent 情绪维度",
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

    let status =
        run_blocking_future(verify_privilege_auth(connection, agent_id, sender_id, auth_key))?;
    Ok(match status {
        PrivilegeAuthStatus::Elevated { until, record } => {
            match purpose_to_privileged_command(&record.purpose) {
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
                        pending_args: record.pending_args,
                    },
                },
                None => AuthCommandOutcome::Reply(format!(
                    "授权成功。你已进入特权模式，有效期至 {until}。"
                )),
            }
        }
        PrivilegeAuthStatus::NotFound => AuthCommandOutcome::Reply(
            "当前没有待验证的授权密钥，请重新触发需要特权的命令。".to_string(),
        ),
        PrivilegeAuthStatus::Pending(_) => {
            AuthCommandOutcome::Reply("当前密钥仍待验证，请重新输入。".to_string())
        }
        PrivilegeAuthStatus::Failed(message) => AuthCommandOutcome::Reply(message),
    })
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
        "emotion" => Some(QqPrivilegedCommand::Emotion),
        "adjust_emotion" => Some(QqPrivilegedCommand::AdjustEmotion),
        _ => None,
    }
}
