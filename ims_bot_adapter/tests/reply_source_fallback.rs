use ims_bot_adapter::adapter::parse_reply_source_messages_from_get_msg_response;
use zihuan_core::ims_bot_adapter::models::message::Message;

#[test]
fn get_msg_reply_source_parser_extracts_image_segments() {
    let response = serde_json::json!({
        "status": "ok",
        "retcode": 0,
        "data": {
            "message_id": 1001,
            "group_id": 3001,
            "message": [
                {
                    "type": "image",
                    "data": {
                        "file": "demo.jpg",
                        "url": "https://example.com/demo.jpg"
                    }
                }
            ]
        }
    });

    let messages = parse_reply_source_messages_from_get_msg_response(&response);

    assert_eq!(messages.len(), 1);
    match &messages[0] {
        Message::Image(image) => {
            assert_eq!(
                image.original_source(),
                Some("https://example.com/demo.jpg")
            );
            assert_eq!(image.name(), Some("demo.jpg"));
        }
        other => panic!("expected image message, got {other:?}"),
    }
}
