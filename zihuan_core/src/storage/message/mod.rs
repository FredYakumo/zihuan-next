pub mod chunking;
pub mod history;
pub mod qq_message_list_rdb;
pub mod record_restore;
pub mod record_write;

pub use chunking::{
    split_content_chunks, truncate_field_if_needed, truncate_optional_field_if_needed,
    AT_TARGET_LIST_MAX_CHARS, CONTENT_MAX_CHARS, GROUP_ID_MAX_CHARS, GROUP_NAME_MAX_CHARS,
    MEDIA_ID_LIST_MAX_CHARS, MEDIA_JSON_MAX_CHARS, MESSAGE_ID_MAX_CHARS, SENDER_ID_MAX_CHARS,
    SENDER_NAME_MAX_CHARS,
};
pub use history::{
    load_group_history, load_user_history, search_message_records, MessageSearchFilters,
    MessageSearchPage,
};
pub use qq_message_list_rdb::persist_qq_message_list;
pub use record_restore::{
    cache_message_snapshot, find_media_in_messages, lookup_message_sender, persist_media_to_record,
    query_media_by_id, rebuild_message_list, rebuild_message_list_from_raw_json,
    register_rdb_pool, register_redis_ref, restore_message_snapshot, CachedMessageSnapshotPayload,
    MessageRestoreSource, RestoredMessageSnapshot,
};
pub use record_write::persist_message_event;
