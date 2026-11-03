use std::sync::Arc;

use serde_json::{json, Value};

use crate::agent::tools::Tool;
use crate::error::{Error, Result};
use crate::model_inference::llm::tooling::FunctionTool;
use crate::storage::MemoryCategory;

use super::function_spec::MemoryFunctionToolSpec;
use super::resources::MemoryAgentResources;
use super::store::search_category;
use super::{category_argument, memory_limit};
use crate::utils::json_utils::optional_string_argument;
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
        Arc::new(MemoryFunctionToolSpec::new("search_memory", "Search concept, person or behavioral memory. Use qq_id for an exact person identity lookup; aliases never prove identity.", json!({"type":"object","properties":{"category":{"type":"string","enum":["concept","person","behavioral"]},"query":{"type":"string"},"qq_id":{"type":"string"},"top_n":{"type":"integer"}},"required":["category"],"additionalProperties":false})))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let category = category_argument(arguments)?;
        let qq_id = optional_string_argument(arguments, "qq_id");
        if qq_id.is_some() && category != MemoryCategory::Person {
            return Err(Error::ValidationError("qq_id requires person category".to_string()));
        }
        let query = qq_id
            .clone()
            .or_else(|| optional_string_argument(arguments, "query"))
            .ok_or_else(|| Error::ValidationError("query or qq_id required".to_string()))?;
        let mut hits = search_category(
            &self.resources.memory_backend,
            self.resources.embedding_model.as_deref(),
            &self.resources.access,
            category,
            Some(&query),
            memory_limit(arguments.get("top_n").and_then(Value::as_i64)) as usize,
        )?;
        if let Some(qq_id) = qq_id {
            hits.retain(|hit| hit.record.attributes.person.qq_id == qq_id);
        }
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
