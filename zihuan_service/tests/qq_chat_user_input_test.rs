use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use ims_bot_adapter::models::event_model::{MessageEvent, MessageType, Sender};
use ims_bot_adapter::models::message::{
    ForwardMessage, ForwardNodeMessage, ImageMessage, Message, PersistedMedia,
    PersistedMediaSource, PlainTextMessage, ReplyMessage,
};
use ims_bot_adapter::REPLAY_CONTENT_LABEL;
use zihuan_core::llm::MessagePart;
use zihuan_service::agent::qq_chat_agent::prepare_message_event_user_input_for_test;

fn build_sender() -> Sender {
    Sender {
        user_id: 2001,
        nickname: "sender".to_string(),
        card: String::new(),
        role: None,
    }
}

fn build_event(message_list: Vec<Message>) -> MessageEvent {
    MessageEvent {
        message_id: 1001,
        message_type: MessageType::Group,
        sender: build_sender(),
        message_list,
        group_id: Some(3001),
        group_name: Some("test-group".to_string()),
        is_group_message: true,
    }
}

fn spawn_test_image_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0u8; 1024];
            let bytes_read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..bytes_read]);
            let is_head = request.starts_with("HEAD ");
            let body = [137u8, 80, 78, 71];
            let response = if is_head {
                "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .as_bytes()
                    .to_vec()
            } else {
                let mut head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                head.extend_from_slice(&body);
                head
            };
            stream.write_all(&response).expect("write response");
        }
    });
    format!("http://{}/image.png", addr)
}

fn build_image_message(name: &str, source: &str) -> Message {
    Message::Image(ImageMessage::new(PersistedMedia::new(
        PersistedMediaSource::Upload,
        source,
        "",
        Some(name.to_string()),
        None,
        Some("image/png".to_string()),
    )))
}

#[test]
fn prepare_user_input_keeps_plain_text_as_single_text_part() {
    let event = build_event(vec![Message::PlainText(PlainTextMessage {
        text: "@bot 你好".to_string(),
    })]);

    let prepared = prepare_message_event_user_input_for_test(&event, "bot", "bot");

    assert_eq!(prepared.text, "你好");
    assert!(!prepared.has_media);
    assert!(prepared.image_reference_lines.is_empty());
    assert_eq!(prepared.parts.len(), 1);
    assert!(matches!(prepared.parts[0], MessagePart::Text { .. }));
    assert!(prepared.is_at_me);
}

#[test]
fn prepare_user_input_turns_image_message_into_media_part() {
    let image_url = spawn_test_image_server();
    let event = build_event(vec![build_image_message("image.png", &image_url)]);

    let prepared = prepare_message_event_user_input_for_test(&event, "bot", "bot");

    assert!(prepared.has_media);
    assert_eq!(prepared.multimodal_stats.image_parts, 1);
    assert_eq!(prepared.image_reference_lines.len(), 1);
    assert!(prepared
        .parts
        .iter()
        .any(|part| matches!(part, MessagePart::Image { .. })));
}

#[test]
fn prepare_user_input_resolves_inline_data_url_into_image_part() {
    let image_url = spawn_test_image_server();
    let event = build_event(vec![Message::PlainText(PlainTextMessage {
        text: format!("看图 {image_url}"),
    })]);

    let prepared = prepare_message_event_user_input_for_test(&event, "bot", "bot");

    assert!(prepared.has_media);
    assert_eq!(prepared.multimodal_stats.image_parts, 1);
    assert_eq!(prepared.parts.len(), 2);
    assert!(matches!(prepared.parts[0], MessagePart::Text { .. }));
    assert!(matches!(prepared.parts[1], MessagePart::Image { .. }));
}

#[test]
fn prepare_user_input_includes_reply_source_text_and_media() {
    let image_url = spawn_test_image_server();
    let reply_source = vec![
        Message::PlainText(PlainTextMessage {
            text: "原消息".to_string(),
        }),
        build_image_message("reply.png", &image_url),
    ];
    let event = build_event(vec![
        Message::Reply(ReplyMessage {
            id: 999,
            message_source: Some(reply_source),
        }),
        Message::PlainText(PlainTextMessage {
            text: "这是回复".to_string(),
        }),
    ]);

    let prepared = prepare_message_event_user_input_for_test(&event, "bot", "bot");

    assert!(prepared.text.contains(REPLAY_CONTENT_LABEL));
    assert!(prepared.text.contains("原消息"));
    assert!(prepared
        .image_reference_lines
        .iter()
        .any(|line| line.contains("media_id=")));
    assert!(prepared
        .parts
        .iter()
        .any(|part| matches!(part, MessagePart::Image { .. })));
}

#[test]
fn prepare_user_input_handles_forward_nested_media() {
    let image_url = spawn_test_image_server();
    let event = build_event(vec![Message::Forward(ForwardMessage {
        id: Some("forward-1".to_string()),
        content: vec![ForwardNodeMessage {
            user_id: Some("3002".to_string()),
            nickname: Some("alice".to_string()),
            id: Some("node-1".to_string()),
            content: vec![
                Message::PlainText(PlainTextMessage {
                    text: "前文".to_string(),
                }),
                build_image_message("forward.png", &image_url),
            ],
        }],
    })]);

    let prepared = prepare_message_event_user_input_for_test(&event, "bot", "bot");

    assert!(prepared.has_media);
    assert!(prepared
        .image_reference_lines
        .iter()
        .any(|line| line.contains("media_id=")));
    assert!(prepared
        .parts
        .iter()
        .any(|part| matches!(part, MessagePart::Image { .. })));
}
