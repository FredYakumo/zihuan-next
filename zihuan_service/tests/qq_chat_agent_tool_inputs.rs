use ims_bot_adapter::models::event_model::{MessageEvent, MessageType, Sender};
use ims_bot_adapter::models::message::{
    ImageMessage, Message, PersistedMedia, PersistedMediaSource, PlainTextMessage, ReplyMessage,
};
use zihuan_service::agent::qq_chat_agent::expand_message_event_for_tool_input;

fn build_reply_image_event() -> MessageEvent {
    let referenced_image = Message::Image(ImageMessage::new(PersistedMedia::new(
        PersistedMediaSource::QqChat,
        "https://example.com/image.jpg",
        "qq-images/tests/reply_source_image.jpg",
        Some("image.jpg".to_string()),
        None,
        Some("image/jpeg".to_string()),
    )));

    MessageEvent {
        message_id: 1002,
        message_type: MessageType::Group,
        sender: Sender {
            user_id: 2001,
            nickname: "sender".to_string(),
            card: String::new(),
            role: None,
        },
        message_list: vec![
            Message::Reply(ReplyMessage {
                id: 1001,
                message_source: Some(vec![referenced_image]),
            }),
            Message::PlainText(PlainTextMessage {
                text: "@bot 这是谁？".to_string(),
            }),
        ],
        group_id: Some(3001),
        group_name: Some("test-group".to_string()),
        is_group_message: true,
    }
}

#[test]
fn tool_input_expands_reply_source_images_into_top_level_message_list() {
    let event = build_reply_image_event();

    let expanded = expand_message_event_for_tool_input(&event);

    assert!(
        matches!(event.message_list.first(), Some(Message::Reply(_))),
        "precondition failed: original event should still begin with reply shell"
    );
    assert!(
        expanded
            .message_list
            .iter()
            .any(|message| matches!(message, Message::Image(_))),
        "expanded tool input should expose referenced images at the top level"
    );
    assert!(
        !expanded
            .message_list
            .iter()
            .any(|message| matches!(message, Message::Reply(_))),
        "expanded tool input should replace reply shells with explicit content"
    );
}
