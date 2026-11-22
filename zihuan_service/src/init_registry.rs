use crate::error::{Error, Result};
use dynamic_script_engine::check_node_runtime;
use log::info;

pub fn init_node_registry() -> Result<()> {
    zihuan_core::graph::registry::init_node_registry_with_extensions(&[
        zihuan_core::storage::init_node_registry,
        zihuan_core::ims_bot_adapter::init_node_registry,
        zihuan_core::model_inference::init_node_registry,
    ])
}

pub async fn start_node_worker() -> Result<()> {
    let workspace_root = std::env::current_dir().map_err(|error| {
        Error::ValidationError(format!("无法获取动态脚本运行时工作目录: {error}"))
    })?;
    let config = zihuan_core::config::ConfigCenter::shared().load_root()?.node_runtime;
    let (_command, version, executable) = check_node_runtime(&workspace_root, &config)
        .await
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    info!("Dynamic Script Runtime ready: {executable} ({version})");
    zihuan_core::graph::script_node::start_dynamic_script_runtime()
}
