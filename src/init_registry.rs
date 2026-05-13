use crate::error::Result;

pub fn init_node_registry() -> Result<()> {
    zihuan_graph_engine::registry::init_node_registry_with_extensions(&[
        storage_handler::init_node_registry,
        ims_bot_adapter::init_node_registry,
        zihuan_llm::init_node_registry,
        zihuan_service::init_node_registry,
    ])
}
