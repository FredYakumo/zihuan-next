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
            "List accessible memory titles by category. With query, use the same hybrid retrieval and exact-name-first relevance order as search_memory; without query, browse by time. An empty result means no matching memory was found.",
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
        Ok(json!({"ok":true,"items":hits.into_iter().map(|hit| {
            let record = hit.record;
            let (title, qq_id) = match &record {
                crate::storage::MemoryRecord::Person(person) => {
                    (person.profile.name.clone(), person.profile.qq_id.clone())
                }
                crate::storage::MemoryRecord::Concept(concept) => (concept.key.clone(), String::new()),
                crate::storage::MemoryRecord::Behavioral(behavioral) => {
                    (behavioral.key.clone(), String::new())
                }
            };
            json!({"object_id":record.object_id(),"title":title,"qq_id":qq_id,"category":record.category(),"access_level":record.access_level()})
        }).collect::<Vec<_>>()}).to_string())
    }
}
