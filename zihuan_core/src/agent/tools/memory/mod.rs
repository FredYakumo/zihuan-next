//! Agent memory: the storage backend, the tools Brain and selected sub agents call, the
//! pure-Rust recall phase, and the typed shape of one turn's recall.
//!
//! The module is split by tool: each file is named after the tool id the model sees, over a
//! small set of shared backend primitives ([`store`]) and the recall model ([`recall`]).
//! The former LLM-driven memory sub agent is gone: recall runs as pure Rust
//! ([`recall_memory`]), and its deep search lives on as [`FullSearchMemoryTool`], which the
//! deep-research sub agent declares in its `tool_ids`.

use std::sync::Arc;

use serde_json::Value;

use crate::agent::declarative_agent::AgentHost;
use crate::error::{Error, Result};
use crate::storage::MemoryCategory;

mod full_search_memory;
mod function_spec;
mod list_memory_keys;
mod recall;
mod resources;
mod search_memory;
mod store;
mod update_memory;

pub use full_search_memory::{
    full_search_memory, FullSearchMemoryTool, FULL_SEARCH_MEMORY_TOOL_ID,
};
pub use list_memory_keys::ListMemoryKeysTool;
pub use recall::{recall_memory, BehavioralMemory, ConceptMemory, MemoryRecall, RecalledMemory};
pub use resources::{MemoryAgentResources, MemoryBackend};
pub use search_memory::SearchMemoryTool;
pub use store::search_category;
pub use update_memory::RememberMemoryTool;

/// Memory tools directly available to Brain and selected sub agents.
pub const MEMORY_TOOL_IDS: [&str; 3] = ["list_memory_keys", "search_memory", "update_memory"];

/// Registers the built-in memory tools on an [`AgentHost`] under the ids referenced by
/// agent definitions: the brain-facing tools plus the deep-research full search.
pub fn register_memory_tools(host: &mut AgentHost, resources: MemoryAgentResources) {
    host.register_tool("list_memory_keys", Arc::new(ListMemoryKeysTool::new(resources.clone())));
    host.register_tool("search_memory", Arc::new(SearchMemoryTool::new(resources.clone())));
    host.register_tool("update_memory", Arc::new(RememberMemoryTool::new(resources.clone())));
    host.register_tool(FULL_SEARCH_MEMORY_TOOL_ID, Arc::new(FullSearchMemoryTool::new(resources)));
}

/// Hits one tool call returns when the caller does not ask for a specific count.
pub(crate) const DEFAULT_MEMORY_TOP_N: i64 = 5;
/// Upper bound on the count a caller may ask for.
const MAX_MEMORY_TOP_N: i64 = 50;

pub(crate) fn category_argument(arguments: &Value) -> Result<MemoryCategory> {
    serde_json::from_value(
        arguments
            .get("category")
            .cloned()
            .ok_or_else(|| Error::ValidationError("category is required".to_string()))?,
    )
    .map_err(Error::from)
}

pub(crate) fn memory_limit(value: Option<i64>) -> i64 {
    value.unwrap_or(DEFAULT_MEMORY_TOP_N).clamp(1, MAX_MEMORY_TOP_N)
}
