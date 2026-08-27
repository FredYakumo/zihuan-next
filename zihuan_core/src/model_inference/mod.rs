pub mod agent_config_support;
pub mod inference_function;
pub mod linalg;
pub mod llm;
pub mod llm_api;
pub mod message_content_utils;
pub mod model_config;
pub mod model_factory;
pub mod nn;
pub mod nodes;

use crate::error::Result;

pub fn init_node_registry() -> Result<()> {
    Ok(())
}
