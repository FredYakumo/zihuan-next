use crate::graph::message_restore::{rebuild_message_list, rebuild_message_list_from_raw_json};
use crate::ims_bot_adapter::models::message::{
    ImageMessage, Message, MessageMediaRecord, PersistedMedia, PersistedMediaSource,
};

/// Purpose: Verify fallback restoration rebuilds image media from serialized media records.
/// TestData: Empty text with one QqChat image `MessageMediaRecord`.
#[test]
fn rebuild_message_list_from_media_json_restores_persisted_media_image() {
    let media_json = serde_json::to_string(&vec![MessageMediaRecord {
        segment_index: 0,
        r#type: "image".to_string(),
        media_id: "media-1".to_string(),
        source: PersistedMediaSource::QqChat,
        original_source: "https://multimedia.nt.qq.com.cn/download?fileid=1".to_string(),
        rustfs_path: "qq-images/2026/05/16/1.jpg".to_string(),
        name: Some("download".to_string()),
        description: Some("图片描述".to_string()),
        mime_type: Some("image/jpeg".to_string()),
    }])
    .expect("serialize media json");
    let messages = rebuild_message_list("", Some(&media_json));
    match &messages[0] {
        Message::Image(image) => {
            assert_eq!(image.media.media_id, "media-1");
            assert_eq!(image.media.rustfs_path, "qq-images/2026/05/16/1.jpg");
            assert_eq!(image.media.mime_type.as_deref(), Some("image/jpeg"));
        }
        other => panic!("expected image message, got {other:?}"),
    }
}

/// Purpose: Verify raw message JSON restoration preserves nested persisted media.
/// TestData: One uploaded PNG `ImageMessage` serialized as a message list.
#[test]
fn rebuild_message_list_from_raw_json_restores_nested_media() {
    let messages = vec![Message::Image(ImageMessage::new(PersistedMedia::new(
        PersistedMediaSource::Upload,
        "upload://manual/demo",
        "uploads/demo.png",
        Some("demo.png".to_string()),
        None,
        Some("image/png".to_string()),
    )))];
    let raw_json = serde_json::to_string(&messages).expect("serialize messages");
    let restored = rebuild_message_list_from_raw_json(&raw_json).expect("restore raw json");
    assert_eq!(restored.len(), 1);
    match &restored[0] {
        Message::Image(image) => assert_eq!(image.media.rustfs_path, "uploads/demo.png"),
        other => panic!("expected image message, got {other:?}"),
    }
}
