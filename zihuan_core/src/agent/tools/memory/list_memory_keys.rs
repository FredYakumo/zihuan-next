use std::sync::Arc;

use serde_json::{json, Value};

use crate::agent::tools::Tool;
use crate::error::Result;
use crate::model_inference::llm::tooling::FunctionTool;
use crate::storage::MemoryQuery;

use super::function_spec::MemoryFunctionToolSpec;
use super::resources::MemoryAgentResources;
use super::store::search_category;
use super::{category_argument, memory_limit};

/// Lists memory titles of one category without loading their content.
pub struct ListMemoryKeysTool {
    resources: MemoryAgentResources,
}

impl ListMemoryKeysTool {
    pub fn new(resources: MemoryAgentResources) -> Self {
        Self { resources }
    }
}

impl Tool for ListMemoryKeysTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new(
            "list_memory_keys",
            "List accessible memory titles by category.",
            json!({"type":"object","properties":{"category":{"type":"string","enum":["concept","person","behavioral"]},"top_n":{"type":"integer"},"query":crate::storage::memory::memory_query_schema()},"required":["category"],"additionalProperties":false}),
        ))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let query: Option<MemoryQuery> =
            arguments.get("query").cloned().map(serde_json::from_value).transpose()?;
        let hits = search_category(
            &self.resources.memory_backend,
            self.resources.embedding_model.as_deref(),
            &self.resources.access,
            category_argument(arguments)?,
            query.as_ref(),
            memory_limit(arguments.get("top_n").and_then(Value::as_i64)) as usize,
        )?;
        Ok(json!({"ok":true,"items":hits.into_iter().map(|hit| json!({"object_id":hit.record.object_id,"title":if hit.record.attributes.category == crate::storage::MemoryCategory::Person { hit.record.attributes.person.name.clone() } else { hit.record.key },"qq_id":hit.record.attributes.person.qq_id,"category":hit.record.attributes.category,"access_level":hit.record.attributes.access_level})).collect::<Vec<_>>()}).to_string())
    }
}
