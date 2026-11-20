use std::sync::Arc;

use serde_json::{json, Value};

use crate::agent::tools::Tool;
use crate::error::Result;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::model_inference::llm::tooling::FunctionTool;
use crate::storage::{AgentMemoryAccessContext, MemoryCategory, MemorySearchHit};

use super::function_spec::MemoryFunctionToolSpec;
use super::resources::{MemoryAgentResources, MemoryBackend};
use crate::utils::memory_fingerprint::memory_fingerprint;

/// Tool id the deep-research sub agent declares in its `tool_ids`.
pub const FULL_SEARCH_MEMORY_TOOL_ID: &str = "full_search_memory";

/// Cross-category explorer search. All categories obey the query; loading
/// scoped behavioral rules for an agent is separate from searching the UI.
pub fn full_search_memory(
    backend: &MemoryBackend,
    embedding_model: Option<&dyn EmbeddingBase>,
    access: &AgentMemoryAccessContext,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<(MemoryCategory, Vec<MemorySearchHit>)>> {
    let typed_query = query.map(|query| crate::storage::MemoryQuery::Query(query.to_string()));
    std::thread::scope(|scope| {
        let tasks = MemoryCategory::ALL
            .into_iter()
            .map(|category| {
                let typed_query = typed_query.as_ref();
                scope.spawn(move || {
                    crate::agent::tools::memory::search_category(
                        backend,
                        embedding_model,
                        access,
                        category,
                        typed_query,
                        limit,
                    )
                    .map(|hits| (category, hits))
                })
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| {
                task.join().map_err(|_| crate::string_error!("memory search worker failed"))?
            })
            .collect::<Result<Vec<_>>>()
    })
}

/// One-call deep search across every memory category, exposed to the deep-research
/// sub agent. Results are raw records, not an LLM summary.
pub struct FullSearchMemoryTool {
    resources: MemoryAgentResources,
}

impl FullSearchMemoryTool {
    pub fn new(resources: MemoryAgentResources) -> Self {
        Self { resources }
    }
}

impl Tool for FullSearchMemoryTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new(
            FULL_SEARCH_MEMORY_TOOL_ID,
            "Deep-search every memory category (concept, person, behavioral) in one call with one query. Hybrid lexical+vector retrieval; exact keys/names rank first, then relevance, not recency. Returns the full records grouped by category, so follow-up targeted searches stay on `search_memory`. Results may be empty.",
            json!({"type":"object","properties":{"query":{"type":"string"},"top_n":{"type":"integer","minimum":1,"maximum":50}},"required":["query"],"additionalProperties":false}),
        ))
    }

    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .ok_or_else(|| crate::string_error!("query required"))?;
        let top_n = arguments
            .get("top_n")
            .and_then(Value::as_i64)
            .unwrap_or(super::DEFAULT_MEMORY_TOP_N)
            .clamp(1, super::MAX_MEMORY_TOP_N) as usize;
        let batches = full_search_memory(
            &self.resources.memory_backend,
            self.resources.embedding_model.as_deref(),
            &self.resources.access,
            Some(query),
            top_n,
        )?;
        let mut grouped = serde_json::Map::new();
        for (category, hits) in batches {
            let items = hits
                .into_iter()
                .map(|hit| {
                    let fingerprint = memory_fingerprint(&hit.record)?;
                    Ok(json!({"fingerprint":fingerprint,"memory":hit.record}))
                })
                .collect::<Result<Vec<_>>>()?;
            grouped.insert(
                serde_json::to_value(category)?.as_str().unwrap().to_string(),
                json!(items),
            );
        }
        Ok(Value::Object(grouped).to_string())
    }
}
