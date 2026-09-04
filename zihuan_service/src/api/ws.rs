use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use log::{info, warn};
use salvo::prelude::*;
use salvo::websocket::{Message, WebSocket, WebSocketUpgrade};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;

use super::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    TaskStarted {
        task_id: String,
        graph_name: String,
        graph_session_id: String,
    },
    TaskFinished {
        task_id: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    TaskStopped {
        task_id: String,
    },
    LogMessage {
        level: String,
        message: String,
        timestamp: String,
    },
    GraphValidationResult {
        graph_id: String,
        issues: Vec<serde_json::Value>,
    },
    NodePreviewQQMessages {
        task_id: String,
        graph_session_id: String,
        node_id: String,
        messages: serde_json::Value,
    },
    NodeUiUpdate {
        task_id: String,
        graph_session_id: String,
        node_id: String,
        revision: u64,
        state: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Subscribe {
        graph_id: String,
    },
    Unsubscribe {
        graph_id: String,
    },
    Ping,
    NodeUiEvent {
        graph_session_id: String,
        task_id: Option<String>,
        node_id: String,
        event: String,
        payload: Value,
    },
}

/// Broadcast sender for server → all WS clients
pub type WsBroadcast = broadcast::Sender<ServerMessage>;

pub fn create_broadcast() -> WsBroadcast {
    let (tx, _) = broadcast::channel(256);
    tx
}

#[handler]
pub async fn ws_handler(
    req: &mut Request,
    res: &mut Response,
    depot: &mut Depot,
) -> Result<(), StatusError> {
    let state = depot.obtain::<Arc<AppState>>().unwrap().clone();
    let broadcast_tx = depot.obtain::<WsBroadcast>().unwrap().clone();

    WebSocketUpgrade::new()
        .upgrade(req, res, move |ws| handle_ws_connection(ws, state, broadcast_tx))
        .await
}

async fn handle_ws_connection(ws: WebSocket, _state: Arc<AppState>, broadcast_tx: WsBroadcast) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let mut broadcast_rx = broadcast_tx.subscribe();

    info!("WebSocket client connected");

    let forward_task = tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(msg) => {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        if ws_tx.send(Message::text(json)).await.is_err() {
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        match result {
            Ok(msg) if msg.is_text() => {
                let text = msg.as_str().unwrap_or_default();
                match serde_json::from_str::<ClientMessage>(text) {
                    Ok(ClientMessage::Ping) => {
                        // Nothing to do; connection alive
                    }
                    Ok(ClientMessage::Subscribe { graph_id }) => {
                        info!("WS client subscribed to graph {}", graph_id);
                    }
                    Ok(ClientMessage::Unsubscribe { graph_id }) => {
                        info!("WS client unsubscribed from graph {}", graph_id);
                    }
                    Ok(ClientMessage::NodeUiEvent {
                        task_id: Some(task_id),
                        node_id,
                        event,
                        payload,
                        ..
                    }) => {
                        zihuan_core::graph::script_node::deliver_dynamic_script_ui_event(
                            &task_id,
                            &node_id,
                            Some(event),
                            payload,
                        );
                    }
                    Ok(ClientMessage::NodeUiEvent { task_id: None, .. }) => {
                        warn!("WS: UI event missing task_id");
                    }
                    Err(e) => {
                        warn!("WS: invalid client message: {}", e);
                    }
                }
            }
            Ok(msg) if msg.is_close() => break,
            Err(e) => {
                warn!("WS error: {}", e);
                break;
            }
            _ => {}
        }
    }

    forward_task.abort();
    info!("WebSocket client disconnected");
}
