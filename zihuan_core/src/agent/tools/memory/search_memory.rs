use std::sync::Arc;

use serde_json::{json, Value};

use crate::agent::tools::Tool;
use crate::error::{Error, Result};
use crate::model_inference::llm::tooling::FunctionTool;
use crate::storage::MemoryQuery;

use super::function_spec::MemoryFunctionToolSpec;
use super::resources::MemoryAgentResources;
use super::store::search_category;
use super::{category_argument, memory_limit};
use crate::utils::memory_fingerprint::memory_fingerprint;

/// Searches one category and returns the full records, each with its content fingerprint.
pub struct SearchMemoryTool {
    resources: MemoryAgentResources,
}

impl SearchMemoryTool {
    pub fn new(resources: MemoryAgentResources) -> Self {
        Self { resources }
    }
}

impl Tool for SearchMemoryTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new("search_memory", "Search an explicitly selected category. query is either {id: QQ_ID} for exact person identity, or {query: text} for hybrid retrieval. Person text searches qq_id, name, preferred_name, aliases and impression; concept/behavioral text searches title and content. Exact names/titles rank first, then relevance, not recency. Results may be empty; distance is a rank-fusion score, not similarity or confidence. Never infer identity from aliases.", json!({"type":"object","properties":{"category":{"type":"string","enum":["concept","person","behavioral"]},"query":crate::storage::memory::memory_query_schema(),"top_n":{"type":"integer"}},"required":["category","query"],"additionalProperties":false})))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let category = category_argument(arguments)?;
        if arguments.get("qq_id").is_some() {
            return Err(Error::ValidationError(
                "use query: {id: ...} instead of qq_id".to_string(),
            ));
        }
        let query: MemoryQuery = serde_json::from_value(
            arguments
                .get("query")
                .cloned()
                .ok_or_else(|| Error::ValidationError("query is required".to_string()))?,
        )?;
        let hits = search_category(
            &self.resources.memory_backend,
            self.resources.embedding_model.as_deref(),
            &self.resources.access,
            category,
            Some(&query),
            memory_limit(arguments.get("top_n").and_then(Value::as_i64)) as usize,
        )?;
        let items = hits
            .into_iter()
            .map(|hit| {
                let fingerprint = memory_fingerprint(&hit.record)?;
                let mut value = serde_json::to_value(hit)?;
                value["fingerprint"] = json!(fingerprint);
                Ok(value)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"ok":true,"items":items}).to_string())
    }
}
