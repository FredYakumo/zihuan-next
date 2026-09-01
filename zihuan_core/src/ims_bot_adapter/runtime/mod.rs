pub mod active_adapter_manager;
pub mod adapter;
pub mod event;
pub mod extract_message_from_event;
pub mod login_info;
pub mod message_helpers;
pub mod models;
pub mod multimodal_image_url;
pub mod profile;
pub mod send_qq_message_batches;
pub mod system_config;
pub mod tools;
pub mod utils;
pub mod ws_action;

use crate::error::Result;

pub use active_adapter_manager::{
    close_runtime_bot_adapter_instance, ensure_active_bot_adapter, get_active_bot_adapter_handle,
    has_active_bot_adapter, initialize_enabled_bot_adapters,
    list_active_bot_adapter_connection_ids, list_runtime_bot_adapter_instances,
    register_active_bot_adapter, stop_active_bot_adapter, sync_enabled_bot_adapters,
};
pub use login_info::{fetch_login_info, fetch_login_info_via_adapter_connection, qq_avatar_url};
pub use profile::{
    profile_from_login_info, resolve_active_or_fallback_bot_profile,
    resolve_active_or_fallback_bot_profile_from_connection, resolve_fallback_bot_profile,
    resolve_fallback_bot_profile_from_connection, QqBotProfile,
};
pub use system_config::{
    build_ims_bot_adapter, load_ims_bot_adapter_connections, parse_ims_bot_adapter_connection,
    save_ims_bot_adapter_connections, BotAdapterConnection, BotAdapterConnectionConfig,
    BotAdapterConnectionKind, BotAdapterConnectionsSection,
};

// Labels for message structure elements used when rendering ims messages
pub const CURRENT_MESSAGE_LABEL: &str = "[Current Message]";
pub const REPLY_MESSAGE_LABEL: &str = "[Reply Message]";
pub const FORWARD_NODE_LABEL: &str = "[Forward Node]";
pub const SENDER_LABEL: &str = "[Sender]";

// Text markers used to delimit nested message structures
pub const REPLY_START_MARKER: &str = "[Reply Message Start]";
pub const REPLY_END_MARKER: &str = "[Reply Message End]";
pub const FORWARD_START_MARKER: &str = "[Forward Message Start]";
pub const FORWARD_END_MARKER: &str = "[Forward Message End]";
pub const NOT_ANY_TEXT_MARKER: &str = "[No Text Content]";
pub const NOT_REPLAY_TEXT_MARKER: &str = "[No Reply]";

// Labels for message content sections used in LLM prompts
pub const REPLAY_CONTENT_LABEL: &str = "[Replay Content]";
pub const FORWARD_CONTENT_LABEL: &str = "[Forward Content]";
pub const IMAGE_ANALYSIS_LABEL: &str = "[Image Analysis]";
pub const QUOTE_CONTENT_APPENDIX_LABEL: &str = "[Quote Content Appendix]";

pub fn init_node_registry() -> Result<()> {
    Ok(())
}
