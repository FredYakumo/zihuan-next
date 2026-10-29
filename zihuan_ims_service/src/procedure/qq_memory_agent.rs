use async_trait::async_trait;
use zihuan_core::agent::tools::memory_tools::{recall_memory, MemoryAgentResources};
use zihuan_core::error::Result;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

use super::GroupMessageWindowOutput;

pub(crate) struct QqMemoryAgent {
    pub resources: MemoryAgentResources,
    pub input: String,
    pub previous_context: String,
}

pub(crate) struct MemoryAgentOutput(pub String, pub MemoryAgentResources, pub String);

#[async_trait]
impl Procedure for QqMemoryAgent {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "memory_agent",
            name: "Memory Agent Procedure phase",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, context: &ProcedureContext) -> Result<ProcedureOutput> {
        // Window turns replace sender history; memories absent from that new context must
        // be recalled again. Never use a process-wide cache across users or groups.
        let previous = if context.find_output::<GroupMessageWindowOutput>().is_some() {
            String::new()
        } else {
            self.previous_context.clone()
        };
        let resources = self.resources.clone();
        let source_level = if let Some(sender) = &resources.access.sender_id {
            zihuan_core::storage::user_permission::memory_source_level(&resources.access, sender)?
        } else {
            3
        };
        let source = serde_json::json!({"kind":"user","sender_id":resources.access.sender_id,"access_level":source_level});
        let input = self.input.clone();
        let recall_resources = resources.clone();
        let recall_input = input.clone();
        let recalled = tokio::task::spawn_blocking(move || {
            recall_memory(recall_resources, &recall_input, previous)
        })
        .await
        .map_err(|error| zihuan_core::string_error!("memory recall failed: {error}"))??;
        Ok(ProcedureOutput::of(MemoryAgentOutput(format!("Authenticated memory request source: {source}\nRecalled memories by category: {recalled}"), resources, input)))
    }
}
