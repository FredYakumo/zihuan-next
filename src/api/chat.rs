use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use salvo::http::header::CONTENT_TYPE;
use salvo::http::{HeaderValue, StatusCode};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::ToolCalls;
use zihuan_core::llm::{MessageRole, OpenAIMessage};
use zihuan_llm::system_config::AgentConfig;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHistoryRecord {
    pub session_id: String,
    pub agent_id: String,
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

    let state = depot.obtain::<std::sync::Arc<crate::api::state::AppState>>().unwrap().clone();

    match execute_chat_stream(state, body).await {
        Ok(sse_body) => {
            res.headers_mut().insert(
                CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream; charset=utf-8"),
            );
            res.render(Text::Plain(sse_body));
        }
        Err(err) => render_unprocessable_entity(res, err.to_string()),
    }
}

#[handler]
pub async fn list_chat_sessions(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    match load_chat_sessions() {
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

async fn execute_chat_stream(
    state: std::sync::Arc<crate::api::state::AppState>,
    body: ChatStreamRequest,
) -> Result<String> {
    let ChatStreamRequest {
        agent_id,
        session_id,
        mut messages,
        stream,
    } = body;
    let running_agent = state
        .agent_manager
        .running_agent(&agent_id)
        .ok_or_else(|| Error::ValidationError(format!("agent '{}' is not running", agent_id)))?;
    let agent = running_agent.agent_config().clone();

    messages = sanitize_messages(messages);
    if messages.is_empty() {
        return Err(Error::ValidationError(
            "messages must not be empty after sanitization".to_string(),
        ));
    }
    let latest_user_message = messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::User))
        .cloned();

    let trace_id = Uuid::new_v4().to_string();
    let output_messages = tokio::task::spawn_blocking({
        let state = state.clone();
        let agent_id = agent_id.clone();
        move || state.agent_manager.infer_agent_response_with_trace(&agent_id, messages)
    })
    .await
    .map_err(|err| Error::StringError(format!("failed to join chat task: {err}")))??;

    let assistant_message = output_messages
        .iter()
        .rev()
        .find(|message| {
            matches!(message.role, MessageRole::Assistant) && message.tool_calls.is_empty()
        })
        .cloned()
        .ok_or_else(|| {
            Error::StringError(format!(
                "agent '{}' did not produce a final assistant message",
                agent.name
            ))
        })?;

    let session_id = session_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let assistant_message_id = format!("msg_{}", Uuid::new_v4().simple());

    persist_chat_records(
        &session_id,
        &agent,
        &trace_id,
        &assistant_message_id,
        latest_user_message.as_ref(),
        &output_messages,
    )?;

    Ok(build_sse_response(
        &session_id,
        &assistant_message_id,
        &assistant_message,
        stream.unwrap_or(true),
    ))
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
            role: role.to_string(),
            content: message.content_text_owned().unwrap_or_default(),
            timestamp: now.clone(),
            stream_index: None,
            trace_id: trace_id.to_string(),
            message_id: if matches!(message.role, MessageRole::Assistant) && message.tool_calls.is_empty() {
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

fn build_sse_response(
    session_id: &str,
    message_id: &str,
    assistant_message: &OpenAIMessage,
    stream_enabled: bool,
) -> String {
    let mut events = Vec::new();
    events.push(json!({
        "type": "start",
        "session_id": session_id,
        "message_id": message_id,
    }));

    let content = assistant_message.content_text_owned().unwrap_or_default();
    if stream_enabled {
        for (index, token) in split_stream_tokens(&content).into_iter().enumerate() {
            events.push(json!({
                "type": "delta",
                "message_id": message_id,
                "index": index,
                "token": token,
            }));
        }
    } else {
        events.push(json!({
            "type": "delta",
            "message_id": message_id,
            "index": 0,
            "token": content,
        }));
    }

    events.push(json!({
        "type": "done",
        "session_id": session_id,
        "message_id": message_id,
    }));

    let mut body = events
        .into_iter()
        .map(|event| format!("data: {}\n\n", event))
        .collect::<String>();
    body.push_str("data: [DONE]\n\n");
    body
}

fn split_stream_tokens(content: &str) -> Vec<String> {
    content.chars().map(|ch| ch.to_string()).collect()
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

fn load_chat_sessions() -> Result<Vec<ChatSessionSummary>> {
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

        let agent_id = read_first_agent_id(&path).ok().flatten();

        sessions.push(ChatSessionSummary {
            session_id: stem.to_string(),
            updated_at,
            agent_id,
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

fn read_first_agent_id(path: &Path) -> Result<Option<String>> {
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
    Ok(Some(record.agent_id))
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

fn render_bad_request(res: &mut Response, message: String) {
    res.status_code(StatusCode::BAD_REQUEST);
    res.render(Json(json!({ "error": message })));
}

fn render_unprocessable_entity(res: &mut Response, message: String) {
    res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
    res.render(Json(json!({ "error": message })));
}

fn render_internal_error(res: &mut Response, err: impl ToString) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(Json(json!({ "error": err.to_string() })));
}
