use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::system_config::application_data_dir;

pub const CHAT_HISTORY_DIR_NAME: &str = "chat_history";
pub const SESSION_TITLE_SUFFIX: &str = ".title.json";

const MAX_TITLE_LEN: usize = 30;

pub fn chat_history_dir() -> Result<PathBuf> {
    Ok(application_data_dir().join(CHAT_HISTORY_DIR_NAME))
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionTitleFile {
    title: String,
}

pub fn session_title_path(session_id: &str) -> Result<PathBuf> {
    if session_id.trim().is_empty() {
        return Err(Error::ValidationError("session_id must not be empty".to_string()));
    }
    Ok(chat_history_dir()?.join(format!("{session_id}{SESSION_TITLE_SUFFIX}")))
}

/// Read the LLM generated title of a chat session, if its sidecar file exists.
pub fn load_session_title(session_id: &str) -> Result<Option<String>> {
    let path = session_title_path(session_id)?;
    if !path.exists() {
        return Ok(None);
    }
    let file: SessionTitleFile =
        serde_json::from_reader(OpenOptions::new().read(true).open(path).map_err(|err| {
            crate::string_error!("failed to open chat session title file: {err}")
        })?)
        .map_err(|err| crate::string_error!("failed to parse chat session title file: {err}"))?;
    Ok(Some(file.title))
}

/// Persist the LLM generated title of a chat session next to its history file.
pub fn write_session_title(session_id: &str, title: &str) -> Result<()> {
    let path = session_title_path(session_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
    serde_json::to_writer(&mut file, &SessionTitleFile { title: title.to_string() })
        .map_err(|err| crate::string_error!("failed to serialize chat session title: {err}"))?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn delete_session_title(session_id: &str) -> Result<()> {
    let path = session_title_path(session_id)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Normalize a raw LLM generated title into a usable display title.
///
/// Returns `None` when nothing usable remains (empty text or a transport error string).
pub fn sanitize_session_title(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || crate::model_inference::message_content_utils::is_transport_error(trimmed)
    {
        return None;
    }
    let without_quotes = trimmed
        .strip_prefix(['"', '\'', '“', '‘', '「', '《'])
        .and_then(|value| value.strip_suffix(['"', '\'', '”', '’', '」', '》']))
        .unwrap_or(trimmed);
    let collapsed = without_quotes.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.chars().count() <= MAX_TITLE_LEN {
        return Some(collapsed);
    }
    Some(collapsed.chars().take(MAX_TITLE_LEN).collect())
}
