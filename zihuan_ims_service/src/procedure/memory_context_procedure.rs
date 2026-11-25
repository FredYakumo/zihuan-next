use async_trait::async_trait;
use zihuan_core::agent::tools::memory::MemoryRecall;
use zihuan_core::agent::tools::memory::{recall_memory, MemoryAgentResources};
use zihuan_core::error::Result;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

use super::GroupMessageWindowOutput;
use crate::qq_chat::logging::QqChatTurnIdentity;

pub(crate) struct MemoryContextProcedure {
    /// The turn this recall phase belongs to, rebound on the blocking thread so its
    /// embedding inferences report to the turn's context view.
    pub identity: QqChatTurnIdentity,
    pub resources: MemoryAgentResources,
    pub input: String,
    pub source: serde_json::Value,
    pub previous_context: String,
    pub bot_id: String,
    pub identity_context: Option<String>,
}

/// The recall phase's output.
///
/// The recalled memories travel typed by category; the turn assembly renders them, so no
/// downstream stage parses recall JSON. `resources` and `input` are kept so the brain can run
/// the recall again when it compacts the turn's history.
pub(crate) struct MemoryContextOutput {
    pub(crate) memories: MemoryRecall,
    pub(crate) resources: MemoryAgentResources,
    pub(crate) input: String,
    pub(crate) identity_context: String,
    pub(crate) identity_context_is_new: bool,
    pub(crate) bot_id: String,
}

pub(crate) const IDENTITY_MEMORY_LABEL: &str = "[QQ Identity Memories]";

/// Name the stages emitted by this procedure carry, so the context view can group them.
pub(crate) const MEMORY_CONTEXT_PROCEDURE: &str = "MemoryContextProcedure";

/// Reads the sender's and the bot's person memories. Returns the `[QQ Identity Memories]`
/// context and the sender's name, preferred name and aliases (empty when the sender has no
/// accessible person memory), which the recall also searches with.
pub(crate) fn identity_memory_context(
    resources: &MemoryAgentResources,
    bot_id: &str,
) -> Result<(String, Vec<String>)> {
    let mut people = Vec::new();
    let mut sender_terms = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (role, id) in [
        ("sender", resources.access.sender_id.as_deref().unwrap_or_default()),
        ("ROLE_SELF", bot_id),
    ] {
        if id.is_empty() || !seen.insert(id) {
            continue;
        }
        let person = resources.memory_backend.get_person(id)?;
        let person = person.filter(|record| {
            zihuan_core::storage::memory_is_accessible(
                &zihuan_core::storage::MemoryRecord::Person(record.clone()),
                &resources.access,
            )
        });
        if role == "sender" {
            if let Some(record) = &person {
                let profile = &record.profile;
                sender_terms.extend(
                    [&profile.name, &profile.preferred_name]
                        .into_iter()
                        .chain(profile.aliases.iter())
                        .map(|term| term.trim().to_string())
                        .filter(|term| !term.is_empty()),
                );
            }
        }
        people.push(serde_json::json!({"role":role,"qq_id":id,"memory":person.map(|record| record.profile)}));
    }
    Ok((
        format!("{IDENTITY_MEMORY_LABEL}\n{}", serde_json::to_string(&people)?),
        sender_terms,
    ))
}

pub(crate) fn saved_identity_context(
    history: &[zihuan_core::model_inference::llm::LLMMessage],
) -> Option<String> {
    history
        .iter()
        .rev()
        .flat_map(|message| message.parts.iter())
        .find_map(|part| match part {
            zihuan_core::model_inference::llm::MessagePart::Text { text }
                if text.starts_with(IDENTITY_MEMORY_LABEL) =>
            {
                Some(text.clone())
            }
            _ => None,
        })
}

#[async_trait]
impl Procedure for MemoryContextProcedure {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "memory_context_procedure",
            name: MEMORY_CONTEXT_PROCEDURE,
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
        let source = self.source.clone();
        let saved = self.identity_context.clone();
        let bot_id = self.bot_id.clone();
        let input = self.input.clone();
        let recall_input = input.clone();
        let recall_resources = resources.clone();
        let identity = self.identity.clone();
        let trace = zihuan_core::qq_context::current_turn_trace()
            .unwrap_or_else(|| zihuan_core::qq_context::TurnTraceContext {
                agent_id: identity.agent_id.clone(),
                sender_id: identity.sender_id.clone(),
                turn_id: identity.turn_id.clone(),
                path: Vec::new(),
                procedure: None,
            })
            .with_procedure(MEMORY_CONTEXT_PROCEDURE);
        // Announce the recall before the blocking search so the context view shows it while it
        // runs; the stable id lets the result replace the running entry in place.
        zihuan_core::qq_context::emit_qq_context_stage_with_id(
            &self.identity.agent_id,
            &self.identity.sender_id,
            &self.identity.turn_id,
            "记忆检索",
            "memory-recall",
            serde_json::json!({
                "status": "running",
                "query": &input,
                "source": &source,
                "procedure": MEMORY_CONTEXT_PROCEDURE,
            }),
        );
        let (recalled, identity_context, identity_context_is_new) =
            tokio::task::spawn_blocking(move || {
                let _turn_trace = zihuan_core::qq_context::TurnTraceGuard::from_context(trace);
                // Always read the profiles from the store; a saved copy in the history only
                // decides whether they need injecting again.
                let (profiles, identity_terms) =
                    identity_memory_context(&recall_resources, &bot_id)?;
                let identity_is_new = saved.as_deref() != Some(profiles.as_str());
                Ok::<_, zihuan_core::error::Error>((
                    recall_memory(recall_resources, &recall_input, previous, &identity_terms)?,
                    profiles,
                    identity_is_new,
                ))
            })
            .await
            .map_err(|error| zihuan_core::string_error!("memory recall failed: {error}"))??;
        let memories = recalled.with_source(source.to_string());
        // The recall has no relevance scores or threshold any more; report the typed entries so
        // the context view shows what this phase actually put into the turn.
        let mut detail = crate::agent::utils::memory_recall_trace(&memories);
        if let Some(object) = detail.as_object_mut() {
            object.insert("status".to_string(), serde_json::json!("done"));
            object.insert("query".to_string(), serde_json::json!(input.clone()));
            object.insert("source".to_string(), source);
            object.insert("procedure".to_string(), serde_json::json!(MEMORY_CONTEXT_PROCEDURE));
        }
        zihuan_core::qq_context::emit_qq_context_stage_with_id(
            &self.identity.agent_id,
            &self.identity.sender_id,
            &self.identity.turn_id,
            "记忆检索",
            "memory-recall",
            detail,
        );
        Ok(ProcedureOutput::of(MemoryContextOutput {
            memories,
            resources,
            input,
            identity_context,
            identity_context_is_new,
            bot_id: self.bot_id.clone(),
        }))
    }
}
