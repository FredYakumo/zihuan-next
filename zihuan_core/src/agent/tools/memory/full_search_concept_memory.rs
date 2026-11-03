use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::agent::tools::Tool;
use crate::error::Result;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::model_inference::llm::tooling::FunctionTool;
use crate::storage::{AgentMemoryAccessContext, AgentMemorySearchHit, MemoryCategory};
use crate::utils::json_utils::required_string_argument;

use super::function_spec::MemoryFunctionToolSpec;
use super::recall::{MemoryRecall, RecalledMemory};
use super::resources::{MemoryAgentResources, MemoryBackend};
use super::store::search_category;
use crate::utils::memory_fingerprint::memory_fingerprint;

/// FullSearchConceptMemory and the memory explorer share the same parallel recall.
/// Behavioral rules are loaded by scope; concept and person memory use the query.
pub fn full_search_memory(
    backend: &MemoryBackend,
    embedding_model: Option<&dyn EmbeddingBase>,
    access: &AgentMemoryAccessContext,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<(MemoryCategory, Vec<AgentMemorySearchHit>)>> {
    std::thread::scope(|scope| {
        let tasks = MemoryCategory::ALL
            .into_iter()
            .map(|category| {
                scope.spawn(move || {
                    search_category(
                        backend,
                        embedding_model,
                        access,
                        category,
                        if category == MemoryCategory::Behavioral {
                            None
                        } else {
                            query
                        },
                        if category == MemoryCategory::Behavioral {
                            100
                        } else {
                            limit
                        },
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

/// Shared only with the recall phase. Results are raw records, not an LLM summary.
///
/// Accepted hits are recorded twice: verbatim in the JSON the recall agent's tool loop sees, and
/// in their typed class in `results`, which is what the turn's memory hint is built from.
pub struct FullSearchConceptMemory {
    resources: MemoryAgentResources,
    seen: Mutex<HashSet<String>>,
    previous_context: String,
    results: Arc<Mutex<MemoryRecall>>,
}

impl FullSearchConceptMemory {
    pub fn new(
        resources: MemoryAgentResources,
        previous_context: String,
        results: Arc<Mutex<MemoryRecall>>,
    ) -> Self {
        Self {
            resources,
            seen: Mutex::new(HashSet::new()),
            previous_context,
            results,
        }
    }
}

impl Tool for FullSearchConceptMemory {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new("FullSearchConceptMemory", "Search all memory categories in parallel. Use force_check when the user disputes recalled information.",
         json!({"type":"object","properties":{"query":{"type":"string"},"force_check":{"type":"boolean"}},"required":["query"],"additionalProperties":false})))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let query = required_string_argument(arguments, "query")?;
        let force = arguments.get("force_check").and_then(Value::as_bool).unwrap_or(false);
        let batches = full_search_memory(
            &self.resources.memory_backend,
            self.resources.embedding_model.as_deref(),
            &self.resources.access,
            Some(&query),
            10,
        )?;
        let mut grouped = serde_json::Map::new();
        let mut seen = self.seen.lock().unwrap();
        let mut recall = self.results.lock().unwrap();
        for (category, hits) in batches {
            let mut items = Vec::new();
            for hit in hits {
                let fingerprint = memory_fingerprint(&hit.record)?;
                if force
                    || (!seen.contains(&fingerprint)
                        && !self.previous_context.contains(&serde_json::to_string(&fingerprint)?))
                {
                    seen.insert(fingerprint.clone());
                    recall.push(RecalledMemory::from_record(category, &hit.record));
                    items.push(json!({"fingerprint":fingerprint,"memory":hit.record}));
                }
            }
            grouped.insert(
                serde_json::to_value(category)?.as_str().unwrap().to_string(),
                json!(items),
            );
        }
        let result = Value::Object(grouped);
        Ok(result.to_string())
    }
}
