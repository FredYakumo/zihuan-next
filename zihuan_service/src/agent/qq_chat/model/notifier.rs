use std::sync::Arc;

use zihuan_core::command::{CommandContext, NewConversationRequest};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::Result;
use zihuan_graph_engine::data_value::LLMMessageSessionCacheRef;

/// Context for executing command side effects in QQ chat.
pub(crate) struct QqCommandSideEffectContext<'a> {
    pub(crate) command_context: &'a CommandContext,
    pub(crate) cache: &'a Arc<LLMMessageSessionCacheRef>,
    pub(crate) adapter: &'a ims_bot_adapter::adapter::SharedBotAdapter,
    pub(crate) bot_id: &'a str,
    pub(crate) bot_name: &'a str,
    pub(crate) target_id: &'a str,
    pub(crate) is_group: bool,
    pub(crate) group_name: Option<&'a str>,
    pub(crate) rdb_pool: Option<&'a RelationalDbConnection>,
}

/// Notifier for long-running tasks that sends progress updates to QQ chat.
pub(crate) struct QqLongTaskNotifier {
    pub(crate) adapter: ims_bot_adapter::adapter::SharedBotAdapter,
    pub(crate) target_id: String,
    pub(crate) sender_id: String,
    pub(crate) is_group: bool,
    pub(crate) rdb_pool: Option<RelationalDbConnection>,
    pub(crate) group_name: Option<String>,
    pub(crate) bot_id: String,
    pub(crate) bot_name: String,
}
