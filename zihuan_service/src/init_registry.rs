use crate::error::Result;

pub fn init_node_registry() -> Result<()> {
    zihuan_core::graph::registry::init_node_registry_with_extensions(&[
        zihuan_core::storage::init_node_registry,
        zihuan_core::ims_bot_adapter::init_node_registry,
        zihuan_core::model_inference::init_node_registry,
        zihuan_service::init_node_registry,
    ])
}
