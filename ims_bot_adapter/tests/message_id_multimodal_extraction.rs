use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use ims_bot_adapter::adapter::{BotAdapter, BotAdapterConfig};
use ims_bot_adapter::extract_message_by_id_from_event::ExtractMessageByIdFromEventNode;
use ims_bot_adapter::models::event_model::{MessageEvent, MessageType, Sender};
use ims_bot_adapter::models::message::{
    ImageMessage, Message, PersistedMedia, PersistedMediaSource, PlainTextMessage,
};
use zihuan_core::llm::model::message::{ContentPart, MessageContent};
use zihuan_graph_engine::message_restore::cache_message_snapshot;
use zihuan_graph_engine::{DataType, DataValue, Node};

fn build_adapter_handle() -> zihuan_core::ims_bot_adapter::BotAdapterHandle {
    let runtime = tokio::runtime::Runtime::new().expect("create runtime");
    let shared = runtime.block_on(async {
        BotAdapter::new(BotAdapterConfig::new(
            "ws://localhost",
            "token",
            "2496875785",
        ))
        .await
        .into_shared()
    });
    shared
}

fn create_temp_image_path() -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("current time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-test-{unique}.jpg"));
    fs::write(&path, [0xFF, 0xD8, 0xFF, 0xD9]).expect("write temp image");
    path.to_string_lossy().to_string()
}

fn build_message_event(message_id: i64, messages: Vec<Message>) -> MessageEvent {
    MessageEvent {
        message_id,
        message_type: MessageType::Group,
        sender: Sender {
            user_id: 2001,
            nickname: "sender".to_string(),
            card: String::new(),
            role: None,
        },
        message_list: messages,
        group_id: Some(3001),
        group_name: Some("test-group".to_string()),
        is_group_message: true,
    }
}

fn spawn_image_http_server(path: &str, content_type: &str, body: &'static [u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let address = listener.local_addr().expect("listener address");
    let route = path.to_string();
    let content_type = content_type.to_string();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 2048];
            let bytes_read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..bytes_read]);
            assert!(request.starts_with("GET "));
            assert!(request.contains(&route));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                content_type,
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response headers");
            stream.write_all(body).expect("write response body");
        }
    });
    format!("http://{address}{path}")
}

#[test]
fn extract_message_by_id_node_builds_multimodal_parts_from_cached_message() {
    let image_path = create_temp_image_path();
    let target_event = build_message_event(
        4242,
        vec![Message::Image(ImageMessage::new(PersistedMedia::new(
            PersistedMediaSource::QqChat,
            image_path.clone(),
            String::new(),
            Some("cached.jpg".to_string()),
            None,
            Some("image/jpeg".to_string()),
        )))],
    );
    cache_message_snapshot(&target_event);

    let current_event = build_message_event(
        5000,
        vec![Message::PlainText(PlainTextMessage {
            text: "@2496875785 这是什么".to_string(),
        })],
    );

    let mut node = ExtractMessageByIdFromEventNode::new("__test__", "__test__");
    let outputs = node
        .execute(HashMap::from([
            (
                "message_event".to_string(),
                DataValue::MessageEvent(current_event),
            ),
            (
                "ims_bot_adapter".to_string(),
                DataValue::BotAdapterRef(build_adapter_handle()),
            ),
            ("message_id".to_string(), DataValue::Integer(4242)),
        ]))
        .expect("execute node");

    let messages = match outputs.get("messages") {
        Some(DataValue::Vec(inner, items)) if **inner == DataType::OpenAIMessage => items,
        other => panic!("expected Vec<OpenAIMessage>, got {other:?}"),
    };
    let user_message = match &messages[0] {
        DataValue::OpenAIMessage(message) => message,
        other => panic!("expected OpenAIMessage, got {other:?}"),
    };

    match user_message.content.as_ref() {
        Some(MessageContent::Parts(parts)) => {
            assert!(
                parts
                    .iter()
                    .any(|part| matches!(part, ContentPart::ImageUrl { .. })),
                "expected multimodal image part, got {parts:?}"
            );
        }
        other => panic!("expected multipart user content, got {other:?}"),
    }

    let _ = fs::remove_file(image_path);
}

#[test]
fn extract_message_by_id_node_resolves_plain_text_image_url_from_cached_message() {
    let image_url = spawn_image_http_server("/demo.png", "image/png", &[0x89, 0x50, 0x4E, 0x47]);
    let target_event = build_message_event(
        4243,
        vec![Message::PlainText(PlainTextMessage {
            text: format!("请分析这张图 {image_url}"),
        })],
    );
    cache_message_snapshot(&target_event);

    let current_event = build_message_event(
        5001,
        vec![Message::PlainText(PlainTextMessage {
            text: "@2496875785 看图".to_string(),
        })],
    );

    let mut node = ExtractMessageByIdFromEventNode::new("__test__", "__test__");
    let outputs = node
        .execute(HashMap::from([
            (
                "message_event".to_string(),
                DataValue::MessageEvent(current_event),
            ),
            (
                "ims_bot_adapter".to_string(),
                DataValue::BotAdapterRef(build_adapter_handle()),
            ),
            ("message_id".to_string(), DataValue::Integer(4243)),
        ]))
        .expect("execute node");

    let messages = match outputs.get("messages") {
        Some(DataValue::Vec(inner, items)) if **inner == DataType::OpenAIMessage => items,
        other => panic!("expected Vec<OpenAIMessage>, got {other:?}"),
    };
    let user_message = match &messages[0] {
        DataValue::OpenAIMessage(message) => message,
        other => panic!("expected OpenAIMessage, got {other:?}"),
    };

    match user_message.content.as_ref() {
        Some(MessageContent::Parts(parts)) => {
            assert!(
                matches!(parts.first(), Some(ContentPart::Text { text }) if text.contains("请分析这张图"))
            );
            assert!(parts
                .iter()
                .any(|part| matches!(part, ContentPart::ImageUrl { .. })));
        }
        other => panic!("expected multipart user content, got {other:?}"),
    }
}

#[test]
fn extract_message_by_id_node_keeps_non_image_url_as_text() {
    let target_event = build_message_event(
        4244,
        vec![Message::PlainText(PlainTextMessage {
            text: "这个不是图片 https://example.com/index.html".to_string(),
        })],
    );
    cache_message_snapshot(&target_event);

    let current_event = build_message_event(
        5002,
        vec![Message::PlainText(PlainTextMessage {
            text: "@2496875785 看这个".to_string(),
        })],
    );

    let mut node = ExtractMessageByIdFromEventNode::new("__test__", "__test__");
    let outputs = node
        .execute(HashMap::from([
            (
                "message_event".to_string(),
                DataValue::MessageEvent(current_event),
            ),
            (
                "ims_bot_adapter".to_string(),
                DataValue::BotAdapterRef(build_adapter_handle()),
            ),
            ("message_id".to_string(), DataValue::Integer(4244)),
        ]))
        .expect("execute node");

    let messages = match outputs.get("messages") {
        Some(DataValue::Vec(inner, items)) if **inner == DataType::OpenAIMessage => items,
        other => panic!("expected Vec<OpenAIMessage>, got {other:?}"),
    };
    let user_message = match &messages[0] {
        DataValue::OpenAIMessage(message) => message,
        other => panic!("expected OpenAIMessage, got {other:?}"),
    };

    match user_message.content.as_ref() {
        Some(MessageContent::Text(text)) => {
            assert!(text.contains("https://example.com/index.html"));
        }
        other => panic!("expected text content, got {other:?}"),
    }
}
