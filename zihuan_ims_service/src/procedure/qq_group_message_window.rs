//! The group message window agent: picks the group messages related to the user's input.
//!
//! On a group turn with user input the brain's history is not the per-sender conversation.
//! A dedicated agent looks at the most recent `group_message_window_count` group messages
//! (ordered newest to oldest), selects the ones related to the user's input, and those
//! selected messages — time, sender, content — replace the history the brain receives,
//! together with the instruction to reply to the user's input.
//!
//! Messages already selected as related in an earlier turn are remembered per group and
//! never re-enter the candidates, so a related message reaches the brain once; messages the
//! agent judged unrelated stay in the window and remain eligible for later turns.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use log::warn;
use serde_json::json;

use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::Result;
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::{InferenceParam, LLMMessage};
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};
use zihuan_core::storage::message::history::{load_group_history_records, MessageHistoryRecord};
use zihuan_core::utils::string_utils::shorten_text;

use crate::qq_chat::logging::{QqChatTaskTrace, QqChatTurnIdentity};

const LOG_PREFIX: &str = "[GroupMessageWindow]";
const HISTORY_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
/// Per-group cap on remembered confirmed message ids. The window only covers the most
/// recent messages, so ids older than any window can be dropped safely.
const CONFIRMED_IDS_CAP_PER_GROUP: usize = 500;

/// The window display preview per message in the Data Explorer stage detail.
const WINDOW_PREVIEW_CHARS: usize = 160;

const SELECTION_SYSTEM_PROMPT: &str = "The user will send a chat transcript. The message below contains one target text and n history texts, and you must select the history texts connected to the target text:\n\
1. Target text\n\
2. History texts: the most recent group messages, ordered newest to oldest, each line formatted as [index] [time] sender(sender id): content.\n\n\
Task: find the history texts related to the target text. A history text is related when the target text directly responds to, continues, follows up on, or quotes its content, or when its content is required context to understand the target text. Small talk unrelated to the target text, or history texts whose topic has already ended, are not related.\n\n\
Output requirement: reply with strict JSON only: {\"related\": [index, ...]} using the [index] shown on each message; when no message is related, output {\"related\": []}. Do not output anything else.";

/// Remembers which group messages were already selected as related and delivered to the
/// brain, so they are not selected again by later turns.
#[derive(Default)]
pub(crate) struct GroupMessageWindowMemory {
    confirmed: Mutex<HashMap<String, Vec<String>>>,
}

impl GroupMessageWindowMemory {
    /// The window records not yet confirmed related: previously selected messages never
    /// re-enter the candidates, while messages judged unrelated stay eligible.
    pub(crate) fn unconfirmed<'a>(
        &self,
        group_id: &str,
        records: &'a [MessageHistoryRecord],
    ) -> Vec<&'a MessageHistoryRecord> {
        let confirmed = self.confirmed.lock().unwrap();
        let confirmed = confirmed.get(group_id);
        records
            .iter()
            .filter(|record| {
                !confirmed.is_some_and(|ids| ids.iter().any(|id| *id == record.message_id))
            })
            .collect()
    }

    pub(crate) fn record_confirmed(&self, group_id: &str, message_ids: &[String]) {
        if message_ids.is_empty() {
            return;
        }
        let mut confirmed = self.confirmed.lock().unwrap();
        let ids = confirmed.entry(group_id.to_string()).or_default();
        for message_id in message_ids {
            if !ids.contains(message_id) {
                ids.push(message_id.clone());
            }
        }
        let overflow = ids.len().saturating_sub(CONFIRMED_IDS_CAP_PER_GROUP);
        if overflow > 0 {
            ids.drain(0..overflow);
        }
    }
}

/// One group message selected as related to the user's input.
#[derive(Clone)]
pub(crate) struct RelatedGroupMessage {
    pub(crate) message_id: String,
    pub(crate) send_time: String,
    pub(crate) sender_name: String,
    pub(crate) sender_id: String,
    pub(crate) content: String,
}

/// Output of the group-message-window procedure: the group messages selected as related to
/// the user's input. The brain procedure renders them as its history replacement.
#[derive(Clone)]
pub(crate) struct GroupMessageWindowOutput {
    pub(crate) related: Vec<RelatedGroupMessage>,
    pub(crate) user_input: String,
}

impl GroupMessageWindowOutput {
    /// The history replacement handed to the brain: the selected messages as a
    /// time/sender/content record. `None` when the agent found no related messages,
    /// leaving the turn without history. The reply directive is not part of this block;
    /// it rides on the current turn's user message (see [`Self::reply_instruction`]).
    pub(crate) fn history_message(&self) -> Option<LLMMessage> {
        if self.related.is_empty() {
            return None;
        }
        let mut lines = vec![
            "Here are the group messages related to the user's input (time, sender, content):"
                .to_string(),
        ];
        for message in &self.related {
            lines.push(format!(
                "[{}] {}({}): {}",
                message.send_time, message.sender_name, message.sender_id, message.content
            ));
        }
        Some(LLMMessage::user(lines.join("\n")))
    }

    /// The reply directive the before-brain procedure appends to the current turn's user
    /// message, telling the brain which input to answer.
    pub(crate) fn reply_instruction(&self) -> String {
        format!("Please reply to the user's input message: {}", self.user_input)
    }
}

/// Everything the window agent needs, borrowed from the turn context.
pub(crate) struct GroupMessageWindowContext<'a> {
    pub(crate) trace: &'a QqChatTaskTrace,
    pub(crate) identity: QqChatTurnIdentity,
    /// The preprompt model: selection is a cheap single inference, not the brain loop.
    pub(crate) llm: &'a Arc<dyn LLMBase>,
    pub(crate) rdb_pool: Option<&'a RelationalDbConnection>,
    pub(crate) group_id: String,
    /// The triggering message itself. It is persisted into `message_record` before the turn
    /// runs, so it must be kept out of the window candidates.
    pub(crate) current_message_id: i64,
    pub(crate) user_input: String,
    pub(crate) window_count: usize,
    pub(crate) confirmed: &'a GroupMessageWindowMemory,
}

/// QQ group-message-window procedure: selects the group messages related to the user's
/// input from the most recent window and returns them as the brain's history replacement.
///
/// The result decides what the brain receives as history, so the procedure is `Blocking`
/// and runs before the before-brain procedure in the turn chain. Failures degrade to an
/// empty selection (logged and shown in the context view) instead of failing the turn.
pub(crate) struct QqGroupMessageWindow<'a> {
    context: GroupMessageWindowContext<'a>,
}

impl<'a> QqGroupMessageWindow<'a> {
    pub(crate) fn new(context: GroupMessageWindowContext<'a>) -> Self {
        Self { context }
    }

    /// Runs the whole selection: load the window, filter the candidates, infer the related
    /// messages, and remember the newly confirmed message ids. Returns the output plus the
    /// stage detail; every failure path degrades to an empty selection with a reason.
    fn select(&self) -> (GroupMessageWindowOutput, serde_json::Value) {
        let started_at = Instant::now();
        let user_input = self.context.user_input.trim().to_string();
        let mut detail = json!({
            "user_input": self.context.user_input,
            "window_count": self.context.window_count,
        });
        let mut related: Vec<RelatedGroupMessage> = Vec::new();

        'selection: {
            let Some(rdb_pool) = self.context.rdb_pool else {
                detail["reason"] = json!(
                    "no relational database configured; the group message window is unavailable"
                );
                break 'selection;
            };
            let RelationalDbConnection::MySql(mysql) = rdb_pool else {
                detail["reason"] = json!("the group message window requires MySQL message records");
                break 'selection;
            };
            let window = match load_group_history_records(
                mysql,
                self.context.group_id.clone(),
                self.context.window_count as u32,
            ) {
                Ok(window) => window,
                Err(error) => {
                    warn!("{LOG_PREFIX} failed to load group window messages: {error}");
                    detail["reason"] =
                        json!(format!("failed to load group window messages: {error}"));
                    break 'selection;
                }
            };

            // The triggering message is persisted before the turn runs, and messages
            // confirmed related in an earlier turn never re-enter the candidates.
            let current_message_id = self.context.current_message_id.to_string();
            let candidates: Vec<&MessageHistoryRecord> = self
                .context
                .confirmed
                .unconfirmed(&self.context.group_id, &window)
                .into_iter()
                .filter(|record| record.message_id != current_message_id)
                .collect();
            let excluded_confirmed = window.len().saturating_sub(candidates.len()).saturating_sub(
                window.iter().any(|record| record.message_id == current_message_id) as usize,
            );
            detail["window_size"] = json!(window.len());
            detail["candidates"] = json!(candidates.len());
            detail["excluded_confirmed"] = json!(excluded_confirmed);
            if candidates.is_empty() {
                detail["reason"] = json!("no candidate messages in the window");
                break 'selection;
            }
            if user_input.is_empty() {
                detail["reason"] = json!("user input is empty; skipped related message selection");
                break 'selection;
            }

            let mut window_lines = Vec::with_capacity(candidates.len());
            let mut window_view = Vec::with_capacity(candidates.len());
            for (index, record) in candidates.iter().enumerate() {
                let send_time = record.send_time.format(HISTORY_TIME_FORMAT).to_string();
                let sender = format!("{}({})", record.sender_name, record.sender_id);
                window_lines.push(format!(
                    "[{}] [{}] {sender}: {}",
                    index + 1,
                    send_time,
                    record.content
                ));
                window_view.push(json!({
                    "index": index + 1,
                    "time": send_time,
                    "sender": sender,
                    "content": shorten_text(&record.content, WINDOW_PREVIEW_CHARS),
                }));
            }
            detail["window"] = json!(window_view);

            let user_message = format!(
                "[User's Input Message]\n{user_input}\n\n[Group Window Messages] (newest to oldest)\n{}",
                window_lines.join("\n")
            );
            let selection_messages = vec![
                LLMMessage::system(SELECTION_SYSTEM_PROMPT.to_string()),
                LLMMessage::user(user_message),
            ];
            let response = match self.context.llm.inference(&InferenceParam {
                messages: &selection_messages,
                tools: None,
            }) {
                Ok(response) => response,
                Err(error) => {
                    warn!("{LOG_PREFIX} group message window inference failed: {error}");
                    detail["reason"] =
                        json!(format!("group message window inference failed: {error}"));
                    break 'selection;
                }
            };
            let response_text = response.content_text_owned().unwrap_or_default();
            let indexes = match parse_related_indexes(&response_text, candidates.len()) {
                Ok(indexes) => indexes,
                Err(error) => {
                    warn!("{LOG_PREFIX} group message window output parse failed: {error}");
                    detail["reason"] =
                        json!(format!("failed to parse group message window output: {error}"));
                    break 'selection;
                }
            };

            related = indexes
                .iter()
                .map(|index| {
                    let record = candidates[index - 1];
                    RelatedGroupMessage {
                        message_id: record.message_id.clone(),
                        send_time: record.send_time.format(HISTORY_TIME_FORMAT).to_string(),
                        sender_name: record.sender_name.clone(),
                        sender_id: record.sender_id.clone(),
                        content: record.content.clone(),
                    }
                })
                .collect();
            let selected_ids: Vec<String> =
                related.iter().map(|message| message.message_id.clone()).collect();
            self.context.confirmed.record_confirmed(&self.context.group_id, &selected_ids);

            detail["selected_indexes"] = json!(indexes);
            detail["related"] = json!(related
                .iter()
                .map(|message| {
                    json!({
                        "time": message.send_time,
                        "sender": format!("{}({})", message.sender_name, message.sender_id),
                        "content": message.content,
                    })
                })
                .collect::<Vec<_>>());
            if related.is_empty() {
                detail["reason"] = json!("no window messages are related to the user's input");
            }
        }

        detail["duration_ms"] = json!(started_at.elapsed().as_millis() as u64);
        (GroupMessageWindowOutput { related, user_input }, detail)
    }
}

#[async_trait]
impl Procedure for QqGroupMessageWindow<'_> {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "group_message_window",
            name: "group-message-window",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, _context: &ProcedureContext) -> Result<ProcedureOutput> {
        let (output, detail) = self.select();
        self.context.trace.record_graph_phase("group-message-window", detail.clone());
        let identity = &self.context.identity;
        zihuan_core::qq_context::emit_qq_context_stage(
            &identity.agent_id,
            &identity.sender_id,
            &identity.turn_id,
            "group-message-window",
            detail,
        );
        Ok(ProcedureOutput::of(output))
    }
}

/// Parses the agent's `{"related": [index, ...]}` output into 1-based candidate indexes,
/// deduplicated and in ascending window order. Indexes outside the window are an error.
fn parse_related_indexes(
    content: &str,
    candidate_count: usize,
) -> std::result::Result<Vec<usize>, String> {
    let trimmed = content.trim();
    // The model may wrap the JSON in a markdown fence.
    let json_text = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let json_text = json_text.strip_suffix("```").unwrap_or(json_text).trim();
    let value: serde_json::Value = serde_json::from_str(json_text)
        .map_err(|error| format!("invalid selection json: {error}"))?;
    let entries = value
        .get("related")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "selection json missing related array".to_string())?;

    let mut indexes = Vec::with_capacity(entries.len());
    for entry in entries {
        let index = entry
            .as_u64()
            .or_else(|| entry.as_str().and_then(|text| text.parse().ok()))
            .ok_or_else(|| format!("invalid related index: {entry}"))? as usize;
        if index == 0 || index > candidate_count {
            return Err(format!("related index {index} outside the window"));
        }
        if !indexes.contains(&index) {
            indexes.push(index);
        }
    }
    indexes.sort_unstable();
    Ok(indexes)
}
