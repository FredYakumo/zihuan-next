use async_trait::async_trait;
use zihuan_core::agent::tools::memory::MemoryRecall;
use zihuan_core::agent::tools::memory::{recall_memory, MemoryAgentResources};
use zihuan_core::error::Result;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

use super::GroupMessageWindowOutput;
use crate::qq_chat::logging::QqChatTurnIdentity;

pub(crate) struct QqMemoryAgent {
    /// The turn this recall phase belongs to, rebound on the blocking thread so its model
    /// inferences report to the turn's context view.
    pub identity: QqChatTurnIdentity,
    pub resources: MemoryAgentResources,
    pub input: String,
    pub previous_context: String,
}

/// The recall phase's output.
///
/// The recalled memories travel typed by category; the turn assembly renders them, so no
/// downstream stage parses recall JSON. `resources` and `input` are kept so the brain can run the
/// recall again when it compacts the turn's history.
pub(crate) struct MemoryAgentOutput {
    pub(crate) memories: MemoryRecall,
    pub(crate) resources: MemoryAgentResources,
    pub(crate) input: String,
}

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
        let identity = self.identity.clone();
        let recalled = tokio::task::spawn_blocking(move || {
            let _turn_trace = zihuan_core::qq_context::TurnTraceGuard::enter(
                &identity.agent_id,
                &identity.sender_id,
                &identity.turn_id,
            );
            recall_memory(recall_resources, &recall_input, previous)
        })
        .await
        .map_err(|error| zihuan_core::string_error!("memory recall failed: {error}"))??;
        Ok(ProcedureOutput::of(MemoryAgentOutput {
            memories: recalled.with_source(source.to_string()),
            resources,
            input,
        }))
    }
}
