use crate::error::Result;

pub fn init_node_registry() -> Result<()> {
    zihuan_core::graph_engine::registry::init_node_registry_with_extensions(&[
        zihuan_core::storage_handler::init_node_registry,
        zihuan_core::ims_bot_adapter::init_node_registry,
        zihuan_core::model_inference::init_node_registry,
        crate::init_node_registry,
    ])
}
