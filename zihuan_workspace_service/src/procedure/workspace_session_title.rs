use std::sync::Arc;

use async_trait::async_trait;
use zihuan_core::agent::service_config::{RoleServiceConfig, RoleServiceKind};

use crate::role_config::WorkspaceRoleServiceConfig;
use zihuan_core::chat_history::{sanitize_session_title, write_session_title};
use zihuan_core::error::Result;
use zihuan_core::model_inference::agent_config_support::build_llm_from_ref_id;
use zihuan_core::model_inference::llm::{InferenceParam, LLMMessage};
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

const LOG_PREFIX: &str = "[WorkspaceSessionTitle]";

const SESSION_TITLE_SYSTEM_PROMPT: &str = "你是会话命名助手。根据用户的第一条消息为这次对话生成一个简短的标题。要求：直接输出标题本身，不要任何解释、引号或句末标点；使用用户消息的主要语言；不超过20个字符；概括用户想要做的事情。";

const MAX_PROMPT_USER_TEXT_CHARS: usize = 500;

/// Names a newly created dashboard conversation with the orchestration model.
///
/// **Design:** Runs in the `Background` execution mode on purpose: the generated title never
/// feeds the main inference, so blocking the first reply for a naming round-trip would only add
/// latency. The title becomes visible through its sidecar file once the chat UI reloads the
/// session list; failures keep the previous truncated-first-message title. The model reference
/// is resolved once at collection time — the orchestration model, falling back to the main
/// model — and a procedure is only collected when naming applies.
pub struct WorkspaceSessionTitle {
    llm_ref_id: String,
}

/// Return the session title procedure when it applies to the current work unit.
pub(super) fn collect(
    agent: &RoleServiceConfig,
    context: &ProcedureContext,
) -> Vec<Arc<dyn Procedure>> {
    if let Some(llm_ref_id) = session_title_llm_ref_id(agent, context) {
        vec![Arc::new(WorkspaceSessionTitle { llm_ref_id })]
    } else {
        Vec::new()
    }
}

fn session_title_llm_ref_id(
    agent: &RoleServiceConfig,
    context: &ProcedureContext,
) -> Option<String> {
    if !context.is_new_conversation {
        return None;
    }
    let has_user_text = context
        .latest_user_text
        .as_deref()
        .map(str::trim)
        .is_some_and(|text| !text.is_empty());
    if !has_user_text {
        return None;
    }
    if agent.role_service_type.kind != RoleServiceKind::Workspace {
        return None;
    }
    let config: WorkspaceRoleServiceConfig = match agent.role_service_type.parse_typed_config() {
        Ok(config) => config,
        Err(_) => return None,
    };
    config
        .orchestration_llm_ref_id
        .as_deref()
        .or(config.llm_ref_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[async_trait]
impl Procedure for WorkspaceSessionTitle {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "workspace_session_title",
            name: "会话命名",
            execution: ProcedureExecution::Background,
        }
    }

    async fn run(&self, context: &ProcedureContext) -> Result<ProcedureOutput> {
        let Some(user_text) = context
            .latest_user_text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        else {
            return Ok(ProcedureOutput::none());
        };
        let session_id = context.session_id.clone();
        let llm_ref_id = self.llm_ref_id.clone();
        let user_text = user_text.to_string();

        let generated =
            tokio::task::spawn_blocking(move || generate_session_title(&llm_ref_id, &user_text))
                .await
                .map_err(|err| {
                    zihuan_core::string_error!("session title task join failed: {err}")
                })?;

        match generated {
            Some(title) => {
                write_session_title(&session_id, &title)?;
                log::info!("{LOG_PREFIX} named session {session_id}: {title}");
            }
            None => {
                log::warn!("{LOG_PREFIX} failed to generate a title for session {session_id}");
            }
        }
        Ok(ProcedureOutput::none())
    }
}

/// Run the one-off naming inference on the orchestration model and sanitize its answer.
fn generate_session_title(llm_ref_id: &str, user_text: &str) -> Option<String> {
    let llm = build_llm_from_ref_id(Some(llm_ref_id)).ok()?;
    let messages = vec![
        LLMMessage::system(SESSION_TITLE_SYSTEM_PROMPT),
        LLMMessage::user(user_text.chars().take(MAX_PROMPT_USER_TEXT_CHARS).collect::<String>()),
    ];
    let response = llm.inference(&InferenceParam { messages: &messages, tools: None });
    let text = response.content_text_owned()?;
    sanitize_session_title(&text)
}
