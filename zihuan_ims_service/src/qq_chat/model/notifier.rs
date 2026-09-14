use zihuan_core::data_refs::RelationalDbConnection;

/// Notifier for long-running tasks that sends progress updates to QQ chat.
pub(crate) struct QqLongTaskNotifier {
    pub(crate) adapter: zihuan_core::ims_bot_adapter::adapter::SharedBotAdapter,
    pub(crate) target_id: String,
    pub(crate) sender_id: String,
    pub(crate) is_group: bool,
    pub(crate) rdb_pool: Option<RelationalDbConnection>,
    pub(crate) group_name: Option<String>,
    pub(crate) bot_id: String,
    pub(crate) bot_name: String,
}
