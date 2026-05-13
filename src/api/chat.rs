use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ims_bot_adapter::{parse_ims_bot_adapter_connection, qq_avatar_url};
use salvo::http::body::BodySender;
use salvo::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use salvo::http::HeaderValue;
use salvo::http::ResBody;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use storage_handler::{ConnectionConfig, ConnectionKind};
use tokio::sync::mpsc;
use uuid::Uuid;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::ToolCalls;
use zihuan_core::llm::{MessageRole, OpenAIMessage};
use zihuan_core::agent_config::QqChatAgentConfig;
use zihuan_llm::system_config::{AgentConfig, AgentType};

const CHAT_HISTORY_DIR_NAME: &str = "chat_history";
const APP_DIR_NAME: &str = "zihuan-next_aibot";

#[derive(Debug, Deserialize)]
pub struct ChatStreamRequest {
    pub agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub messages: Vec<OpenAIMessage>,
    #[serde(default)]
    pub stream: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ChatSessionSummary {
    pub session_id: String,
    pub updated_at: String,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub agent_type: Option<String>,
    pub agent_avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHistoryRecord {
    pub session_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub agent_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_avatar_url: Option<String>,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    #[serde(default)]
    pub stream_index: Option<usize>,
    pub trace_id: String,
    pub message_id: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCalls>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone)]
struct AgentSnapshot {
    name: String,
    agent_type: String,
    avatar_url: Option<String>,
}

fn extract_agent_snapshot(agent: &AgentConfig, connections: &[ConnectionConfig]) -> AgentSnapshot {
    let agent_type = match &agent.agent_type {
        AgentType::QqChat(_) => "qq_chat",
        AgentType::HttpStream(_) => "http_stream",
    };

    let avatar_url = match &agent.agent_type {
        AgentType::QqChat(config) => resolve_qq_avatar_url(connections, config),
        AgentType::HttpStream(_) => None,
    };

    AgentSnapshot {
        name: agent.name.clone(),
        agent_type: agent_type.to_string(),
        avatar_url,
    }
}

fn resolve_qq_avatar_url(
    connections: &[ConnectionConfig],
    config: &QqChatAgentConfig,
) -> Option<String> {
    let connection = connections
        .iter()
        .find(|item| item.id == config.ims_bot_adapter_connection_id)?;
    let ConnectionKind::BotAdapter(raw) = &connection.kind else {
        return None;
    };
    let bot_connection = parse_ims_bot_adapter_connection(raw).ok()?;
    let qq_id = bot_connection.qq_id.as_ref()?.trim();
    if qq_id.is_empty() {
        return None;
    }
    qq_avatar_url(qq_id)
}

#[handler]
pub async fn stream_chat(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let body: ChatStreamRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => {
            render_bad_request(res, format!("invalid request body: {err}"));
            return;
        }
    };

    if body.agent_id.trim().is_empty() {
        render_bad_request(res, "agent_id must not be empty".to_string());
        return;
    }
    if body.messages.is_empty() {
        render_bad_request(res, "messages must not be empty".to_string());
        return;
    }

    let state = depot
        .obtain::<std::sync::Arc<crate::api::state::AppState>>()
        .unwrap()
        .clone();

    let (sender, receiver) = ResBody::channel();
    res.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    res.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    res.body = receiver;

    tokio::spawn(execute_chat_streaming(state, body, sender));
}

#[handler]
pub async fn list_chat_sessions(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let filter_agent_id = req.query::<String>("agent_id");
    match load_chat_sessions(filter_agent_id.as_deref()) {
        Ok(sessions) => res.render(Json(json!({ "sessions": sessions }))),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn get_chat_session_messages(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    if session_id.trim().is_empty() {
        render_bad_request(res, "session_id must not be empty".to_string());
        return;
    }

    match load_chat_session_messages(&session_id) {
        Ok(messages) => res.render(Json(json!({ "messages": messages }))),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn delete_chat_session(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    if session_id.trim().is_empty() {
        render_bad_request(res, "session_id must not be empty".to_string());
        return;
    }

    match delete_chat_session_file(&session_id) {
        Ok(()) => res.render(Json(json!({ "ok": true }))),
        Err(err) => render_internal_error(res, err),
    }
}

async fn execute_chat_streaming(
    state: Arc<crate::api::state::AppState>,
    body: ChatStreamRequest,
    mut sender: BodySender,
) {
    let ChatStreamRequest {
        agent_id,
        session_id,
        mut messages,
        stream,
    } = body;

    let running_agent = match state.agent_manager.running_agent(&agent_id) {
        Some(agent) => agent,
        None => {
            let event =
                json!({ "type": "error", "error": format!("agent '{}' is not running", agent_id) });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
    };
    let agent = running_agent.agent_config().clone();

    let connections = match crate::system_config::load_connections() {
        Ok(c) => c,
        Err(err) => {
            let event = json!({ "type": "error", "error": err.to_string() });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
    };
    let agent_snapshot = extract_agent_snapshot(&agent, &connections);

    messages = sanitize_messages(messages);
    if messages.is_empty() {
        let event =
            json!({ "type": "error", "error": "messages must not be empty after sanitization" });
        let _ = sender.send_data(format!("data: {event}\n\n")).await;
        return;
    }
    let latest_user_message = messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::User))
        .cloned();

    let trace_id = Uuid::new_v4().to_string();
    let session_id = session_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let assistant_message_id = format!("msg_{}", Uuid::new_v4().simple());

    let start_event = json!({
        "type": "start",
        "session_id": session_id,
        "message_id": assistant_message_id,
    });
    if sender
        .send_data(format!("data: {start_event}\n\n"))
        .await
        .is_err()
    {
        return;
    }

    let (token_tx, mut token_rx) = mpsc::unbounded_channel::<String>();

    let inference_handle = tokio::spawn({
        let state = state.clone();
        let agent_id = agent_id.clone();
        async move {
            state
                .agent_manager
                .infer_agent_response_streaming(&agent_id, messages, token_tx)
                .await
        }
    });

    let stream_enabled = stream.unwrap_or(true);

    if stream_enabled {
        while let Some(token) = token_rx.recv().await {
            let delta_event = json!({
                "type": "delta",
                "message_id": assistant_message_id,
                "token": token,
            });
            if sender
                .send_data(format!("data: {delta_event}\n\n"))
                .await
                .is_err()
            {
                break;
            }
        }
    } else {
        let mut full_content = String::new();
        while let Some(token) = token_rx.recv().await {
            full_content.push_str(&token);
        }
        if !full_content.is_empty() {
            let delta_event = json!({
                "type": "delta",
                "message_id": assistant_message_id,
                "index": 0,
                "token": full_content,
            });
            let _ = sender.send_data(format!("data: {delta_event}\n\n")).await;
        }
    }

    let output_messages = match inference_handle.await {
        Ok(Ok(msgs)) => msgs,
        Ok(Err(err)) => {
            let event = json!({ "type": "error", "error": err.to_string() });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
        Err(err) => {
            let event =
                json!({ "type": "error", "error": format!("failed to join chat task: {err}") });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
    };

    if let Err(err) = persist_chat_records(
        &session_id,
        &agent,
        &agent_snapshot,
        &trace_id,
        &assistant_message_id,
        latest_user_message.as_ref(),
        &output_messages,
    ) {
        let event = json!({ "type": "error", "error": err.to_string() });
        let _ = sender.send_data(format!("data: {event}\n\n")).await;
        return;
    }

    let done_event = json!({
        "type": "done",
        "session_id": session_id,
        "message_id": assistant_message_id,
    });
    let _ = sender.send_data(format!("data: {done_event}\n\n")).await;
    let _ = sender.send_data("data: [DONE]\n\n").await;
}

fn sanitize_messages(messages: Vec<OpenAIMessage>) -> Vec<OpenAIMessage> {
    messages
        .into_iter()
        .filter(|message| {
            let has_content = message
                .content_text_owned()
                .is_some_and(|text| !text.trim().is_empty());
            let has_reasoning = message
                .reasoning_content
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty());
            has_content || has_reasoning || !message.tool_calls.is_empty()
        })
        .collect()
}

fn persist_chat_records(
    session_id: &str,
    agent: &AgentConfig,
    agent_snapshot: &AgentSnapshot,
    trace_id: &str,
    assistant_message_id: &str,
    latest_user_message: Option<&OpenAIMessage>,
    output_messages: &[OpenAIMessage],
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    if let Some(user_message) = latest_user_message {
        let user_record = ChatHistoryRecord {
            session_id: session_id.to_string(),
            agent_id: agent.id.clone(),
            agent_name: agent_snapshot.name.clone(),
            agent_type: agent_snapshot.agent_type.clone(),
            agent_avatar_url: agent_snapshot.avatar_url.clone(),
            role: "user".to_string(),
            content: user_message.content_text_owned().unwrap_or_default(),
            timestamp: now.clone(),
            stream_index: None,
            trace_id: trace_id.to_string(),
            message_id: format!("msg_{}", Uuid::new_v4().simple()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        };
        append_history_record(&user_record)?;
    }

    for message in output_messages {
        let role = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
        };
        let record = ChatHistoryRecord {
            session_id: session_id.to_string(),
            agent_id: agent.id.clone(),
            agent_name: agent_snapshot.name.clone(),
            agent_type: agent_snapshot.agent_type.clone(),
            agent_avatar_url: agent_snapshot.avatar_url.clone(),
            role: role.to_string(),
            content: message.content_text_owned().unwrap_or_default(),
            timestamp: now.clone(),
            stream_index: None,
            trace_id: trace_id.to_string(),
            message_id: if matches!(message.role, MessageRole::Assistant)
                && message.tool_calls.is_empty()
            {
                assistant_message_id.to_string()
            } else {
                format!("msg_{}", Uuid::new_v4().simple())
            },
            tool_calls: message.tool_calls.clone(),
            tool_call_id: message.tool_call_id.clone(),
        };
        append_history_record(&record)?;
    }

    Ok(())
}

fn append_history_record(record: &ChatHistoryRecord) -> Result<()> {
    let path = chat_session_file_path(&record.session_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, record)
        .map_err(|err| Error::StringError(format!("failed to serialize chat record: {err}")))?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn load_chat_sessions(filter_agent_id: Option<&str>) -> Result<Vec<ChatSessionSummary>> {
    let dir = chat_history_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };

        let updated_at = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .map(DateTime::<Utc>::from)
            .map(|time| time.to_rfc3339())
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        let first_record = read_first_record(&path).ok().flatten();

        if let Some(filter) = filter_agent_id {
            if first_record.as_ref().map(|r| r.agent_id.as_str()) != Some(filter) {
                continue;
            }
        }

        sessions.push(ChatSessionSummary {
            session_id: stem.to_string(),
            updated_at,
            agent_id: first_record.as_ref().map(|r| r.agent_id.clone()),
            agent_name: first_record.as_ref().map(|r| r.agent_name.clone()),
            agent_type: first_record.as_ref().map(|r| r.agent_type.clone()),
            agent_avatar_url: first_record
                .as_ref()
                .and_then(|r| r.agent_avatar_url.clone()),
        });
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

fn load_chat_session_messages(session_id: &str) -> Result<Vec<ChatHistoryRecord>> {
    let path = chat_session_file_path(session_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = OpenOptions::new().read(true).open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ChatHistoryRecord>(&line) {
            Ok(record) => entries.push(record),
            Err(err) => {
                return Err(Error::StringError(format!(
                    "failed to parse chat record: {err}"
                )))
            }
        }
    }
    Ok(entries)
}

fn read_first_record(path: &Path) -> Result<Option<ChatHistoryRecord>> {
    let file = OpenOptions::new().read(true).open(path)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 || line.trim().is_empty() {
        return Ok(None);
    }

    let record: ChatHistoryRecord = serde_json::from_str(line.trim()).map_err(|err| {
        Error::StringError(format!("failed to parse first chat history record: {err}"))
    })?;
    Ok(Some(record))
}

fn chat_history_dir() -> Result<PathBuf> {
    let root = zihuan_core::system_config::app_data_dir()
        .join(APP_DIR_NAME)
        .join(CHAT_HISTORY_DIR_NAME);
    Ok(root)
}

fn chat_session_file_path(session_id: &str) -> Result<PathBuf> {
    if session_id.trim().is_empty() {
        return Err(Error::ValidationError(
            "session_id must not be empty".to_string(),
        ));
    }
    Ok(chat_history_dir()?.join(format!("{session_id}.jsonl")))
}

fn delete_chat_session_file(session_id: &str) -> Result<()> {
    let path = chat_session_file_path(session_id)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn render_bad_request(res: &mut Response, message: String) {
    res.status_code(StatusCode::BAD_REQUEST);
    res.render(Json(json!({ "error": message })));
}

fn render_internal_error(res: &mut Response, err: impl ToString) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(Json(json!({ "error": err.to_string() })));
}
