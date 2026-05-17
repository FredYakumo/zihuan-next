use std::collections::HashMap;
use std::fs;
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
        BotAdapter::new(BotAdapterConfig::new("ws://localhost", "token", "2496875785"))
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
                parts.iter().any(|part| matches!(part, ContentPart::ImageUrl { .. })),
                "expected multimodal image part, got {parts:?}"
            );
        }
        other => panic!("expected multipart user content, got {other:?}"),
    }

    let _ = fs::remove_file(image_path);
}
