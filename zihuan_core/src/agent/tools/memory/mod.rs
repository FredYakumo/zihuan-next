//! Agent memory: the storage backend, the tools Brain and selected sub agents call, the recall
//! phase, and the typed shape of one turn's recall.
//!
//! The module is split by tool: each file is named after the tool id the model sees, over a
//! small set of shared backend primitives ([`store`]) and the recall model ([`recall`]).

use std::sync::Arc;

use serde_json::Value;

use crate::agent::declarative_agent::AgentHost;
use crate::error::{Error, Result};
use crate::storage::MemoryCategory;

mod function_spec;
mod full_search_concept_memory;
mod list_memory_keys;
mod memory_agent;
mod recall;
mod resources;
mod search_memory;
mod store;
mod titles;
mod update_memory;

pub use full_search_concept_memory::{full_search_memory, FullSearchConceptMemory};
pub use list_memory_keys::ListMemoryKeysTool;
pub use memory_agent::{memory_agent_definition, recall_memory, MEMORY_AGENT_ID};
pub use recall::{BehavioralMemory, ConceptMemory, MemoryRecall, PersonMemory, RecalledMemory};
pub use resources::{MemoryAgentResources, MemoryBackend};
pub use search_memory::SearchMemoryTool;
pub use store::{search_category, write_memory};
pub use titles::{relevant_memory_titles, MemoryTitleHit};
pub use update_memory::RememberMemoryTool;

/// Memory tools directly available to Brain and selected sub agents.
pub const MEMORY_TOOL_IDS: [&str; 3] = ["list_memory_keys", "search_memory", "update_memory"];

/// Registers the built-in memory tools on an [`AgentHost`] under the ids referenced by the
/// memory agent definition.
pub fn register_memory_tools(host: &mut AgentHost, resources: MemoryAgentResources) {
    host.register_tool("list_memory_keys", Arc::new(ListMemoryKeysTool::new(resources.clone())));
    host.register_tool("search_memory", Arc::new(SearchMemoryTool::new(resources.clone())));
    host.register_tool("update_memory", Arc::new(RememberMemoryTool::new(resources)));
}

/// Hits one tool call returns when the caller does not ask for a specific count.
pub(crate) const DEFAULT_MEMORY_TOP_N: i64 = 5;
/// Upper bound on the count a caller may ask for.
const MAX_MEMORY_TOP_N: i64 = 20;

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
