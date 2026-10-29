//! In-process adapter with filesystem IPC for service-level simulations.
//! Events use the same OneBot message segments and enter the normal parser/handlers.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::adapter::{BotAdapter, BotAdapterBackend, SharedBotAdapter};
use crate::error::Result;
use crate::utils::json_file::write_json_atomic;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockBotAdapterConfig {
    pub directory: PathBuf,
    #[serde(default)]
    pub responses: HashMap<String, Value>,
    /// Optional user profiles and forward records not observed in the current session.
    #[serde(default)]
    pub profiles: Vec<Value>,
    #[serde(default)]
    pub forwards: HashMap<String, Value>,
}

pub struct MockBotAdapter {
    config: MockBotAdapterConfig,
}

impl MockBotAdapterConfig {
    pub fn login_info(&self, bot_id: &str) -> Value {
        self.responses
            .get("get_login_info")
            .cloned()
            .unwrap_or_else(|| json!({"user_id": bot_id, "nickname": "Mock Bot"}))
    }
}

impl MockBotAdapter {
    pub fn new(config: MockBotAdapterConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl BotAdapterBackend for MockBotAdapter {
    async fn run(&self, adapter: SharedBotAdapter) -> Result<()> {
        let root = &self.config.directory;
        for name in ["inbox", "received", "outbox", "actions"] {
            std::fs::create_dir_all(root.join(name))?;
        }
        let bot_id = adapter.lock().await.get_bot_id().parse::<i64>().map_err(|error| {
            crate::string_error!("mock adapter requires a numeric qq_id: {error}")
        })?;
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        adapter.lock().await.action_tx = Some(tx);
        write_json_atomic(&root.join("ready.json"), &json!({"user_id": bot_id}))?;
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(20));
        let mut next_id = 1_000_000_i64;
        let mut messages = HashMap::<i64, Value>::new();
        let mut profiles: HashMap<i64, Value> = self
            .config
            .profiles
            .iter()
            .filter_map(|profile| {
                Some((super::ws_action::json_i64(profile.get("user_id"))?, profile.clone()))
            })
            .collect();
        profiles
            .insert(bot_id, json!({"user_id": bot_id, "nickname": "Mock Bot", "role": "member"}));
        let mut members = HashMap::<i64, HashMap<i64, Value>>::new();
        let mut forwards = self.config.forwards.clone();
        loop {
            tokio::select! {
                Some(payload) = rx.recv() => {
                    let request: Value = serde_json::from_str(&payload)?;
                    let action = request["action"].as_str().unwrap_or_default();
                    let params = &request["params"];
                    let echo = request["echo"].as_str().unwrap_or_default();
                    write_json_atomic(&root.join("actions").join(format!("{echo}.json")), &request)?;
                    let user_id = super::ws_action::json_i64(params.get("user_id"));
                    let group_id = super::ws_action::json_i64(params.get("group_id"));
                    let data = if let Some(data) = self.config.responses.get(action) {
                        Some(data.clone())
                    } else { match action {
                        "get_login_info" => Some(self.config.login_info(&bot_id.to_string())),
                        "get_stranger_info" => user_id.and_then(|id| profiles.get(&id).cloned()),
                        "get_group_member_info" => group_id.and_then(|group| members.get(&group))
                            .and_then(|group| user_id.and_then(|user| group.get(&user).cloned())),
                        "get_group_member_list" => group_id.and_then(|group| members.get(&group))
                            .map(|group| {
                                let mut entries: Vec<_> = group.values().cloned().collect();
                                entries.sort_by_key(|entry| super::ws_action::json_i64(entry.get("user_id")));
                                json!(entries)
                            }),
                        "get_forward_msg" => params["message_id"].as_str().and_then(|id| forwards.get(id).cloned()),
                        "get_msg" => super::ws_action::json_i64(params.get("message_id"))
                            .and_then(|id| messages.get(&id).cloned()),
                        "send_private_msg" | "send_group_msg" | "send_private_forward_msg" | "send_group_forward_msg" => {
                            next_id += 1;
                            let content = if action.ends_with("_forward_msg") {
                                let nodes: Vec<Value> = params["messages"].as_array().into_iter().flatten()
                                    .filter_map(|node| {
                                        let data = &node["data"];
                                        if let Some(id) = super::ws_action::json_i64(data.get("id")) {
                                            messages.get(&id).cloned()
                                        } else {
                                            Some(json!({"sender": {"user_id": data["uin"], "nickname": data["name"]}, "content": data["content"]}))
                                        }
                                    }).collect();
                                forwards.insert(next_id.to_string(), json!({"messages": nodes}));
                                json!([{"type": "forward", "data": {"id": next_id.to_string()}}])
                            } else { params["message"].clone() };
                            let message = json!({
                                "message_id": next_id, "real_id": next_id,
                                "message_type": if group_id.is_some() { "group" } else { "private" },
                                "time": chrono::Utc::now().timestamp(), "message": content,
                                "sender": {"user_id": bot_id, "nickname": "Mock Bot"},
                                "group_id": params["group_id"], "user_id": params["user_id"],
                            });
                            messages.insert(next_id, message);
                            write_json_atomic(&root.join("outbox").join(format!("{next_id}.json")),
                                &json!({"message_id": next_id, "action": action, "params": params}))?;
                            Some(json!({"message_id": next_id, "res_id": next_id.to_string()}))
                        }
                        _ => None,
                    }};
                    let response = match data {
                        Some(data) => json!({"status": "ok", "retcode": 0, "data": data, "echo": echo}),
                        None => json!({"status": "failed", "retcode": 1404, "message": "mock action or message not configured", "echo": echo}),
                    };
                    BotAdapter::process_event(adapter.clone(), response.to_string()).await;
                }
                _ = ticker.tick() => {
                    let mut files = std::fs::read_dir(root.join("inbox"))?
                        .collect::<std::io::Result<Vec<_>>>()?;
                    files.sort_by_key(|entry| entry.file_name());
                    for entry in files {
                        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
                            continue;
                        }
                        let text = std::fs::read_to_string(entry.path())?;
                        let event: Value = serde_json::from_str(&text)?;
                        // Reject invalid fixtures before acknowledging ingestion.
                        let raw: super::models::RawMessageEvent = serde_json::from_value(event.clone())?;
                        profiles.insert(raw.sender.user_id, serde_json::to_value(&raw.sender)?);
                        if let Some(group_id) = raw.group_id {
                            let group = members.entry(group_id).or_default();
                            group.insert(bot_id, profiles[&bot_id].clone());
                            let mut profile = profiles[&raw.sender.user_id].clone();
                            profile["group_id"] = json!(group_id);
                            if profile["role"].is_null() { profile["role"] = json!("member"); }
                            group.insert(raw.sender.user_id, profile);
                        }
                        messages.insert(raw.message_id, event.clone());
                        let receipt = root.join("received").join(entry.file_name());
                        std::fs::remove_file(entry.path())?;
                        let adapter = Arc::clone(&adapter);
                        tokio::spawn(async move {
                            BotAdapter::process_event(adapter, text).await;
                            if let Err(error) = write_json_atomic(&receipt, &event) {
                                log::error!("mock adapter receipt failed: {error}");
                            }
                        });
                    }
                }
            }
        }
    }
}
