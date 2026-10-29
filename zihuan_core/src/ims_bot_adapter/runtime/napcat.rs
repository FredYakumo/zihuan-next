//! Real NapCat WebSocket backend.
use super::adapter::{BotAdapter, BotAdapterBackend, SharedBotAdapter};
use crate::error::Result;
use crate::url_utils::extract_host;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

pub struct NapCatBotAdapter;

#[async_trait]
impl BotAdapterBackend for NapCatBotAdapter {
    async fn run(&self, adapter: SharedBotAdapter) -> Result<()> {
        let (url, token) = {
            let guard = adapter.lock().await;
            (guard.websocket_url().to_string(), guard.get_token().to_string())
        };

        info!("Connecting to bot server at {}", url);

        // Build the WebSocket request with authorization header
        let request = http::Request::builder()
            .uri(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Host", extract_host(&url).unwrap_or("localhost"))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())?;

        let (ws_stream, _) = connect_async(request).await?;
        info!("Connected to the qq bot server successfully.");

        let (mut write, mut read) = ws_stream.split();

        let (action_tx, mut action_rx) = mpsc::unbounded_channel::<String>();
        {
            let mut guard = adapter.lock().await;
            guard.action_tx = Some(action_tx);
        }

        tokio::spawn(async move {
            while let Some(msg) = action_rx.recv().await {
                if write.send(WsMessage::Text(msg)).await.is_err() {
                    break;
                }
            }
        });

        while let Some(msg_result) = read.next().await {
            match msg_result {
                Ok(WsMessage::Text(text)) => {
                    let adapter_clone = adapter.clone();
                    tokio::spawn(async move {
                        BotAdapter::process_event(adapter_clone, text).await;
                    });
                }
                Ok(WsMessage::Binary(data)) => {
                    if let Ok(text) = String::from_utf8(data) {
                        let adapter_clone = adapter.clone();
                        tokio::spawn(async move {
                            BotAdapter::process_event(adapter_clone, text).await;
                        });
                    } else {
                        warn!("Received binary message that is not valid UTF-8");
                    }
                }
                Ok(WsMessage::Close(frame)) => {
                    if let Some(frame) = frame {
                        info!(
                            "WebSocket connection closed by server: code={} reason='{}'",
                            frame.code, frame.reason
                        );
                    } else {
                        info!("WebSocket connection closed");
                    }
                    break;
                }
                Ok(WsMessage::Ping(_)) | Ok(WsMessage::Pong(_)) => {
                    // Heartbeat messages, ignore
                }
                Ok(WsMessage::Frame(_)) => {
                    // Raw frame, ignore
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}
