use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::agent::sub_agent_context::{AgentServiceContext, MemoryCapability};
use zihuan_core::agent::tools::Tool;
use zihuan_core::agent::LLM_KIND_MAIN;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::graph::object_storage::S3Ref;
use zihuan_core::model_inference::llm::embedding_base::EmbeddingBase;
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::rag::WebSearchEngine;
use zihuan_core::retrieval::RetrievalStoreRef;
use zihuan_core::scheduler::JobMemoryResources;
use zihuan_core::storage::AgentMemoryAccessContext;
use zihuan_core::storage::LocalMemoryStore;

mod agent_state;
mod common;
mod editable_qq_agent_tool;
mod image_save;
mod image_search;
mod image_understand;
mod info_tools;
mod qq_message_search;
mod recent_messages;
mod reply_message;
mod web_search;

pub(crate) use agent_state::UpdateAgentStateTool;
pub(crate) use common::{ToolNotificationTarget, QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS};
pub(crate) use editable_qq_agent_tool::EditableQqAgentTool;
pub(crate) use image_save::SaveImageTool;
pub(crate) use image_search::SearchSimilarImagesTool;
pub(crate) use image_understand::{execute_image_understand_tool, ImageUnderstandTool};
pub(crate) use info_tools::{GetAgentPublicInfoTool, GetFunctionListTool};
pub(crate) use qq_message_search::SearchQqMessagesTool;
pub(crate) use recent_messages::{GetRecentGroupMessagesTool, GetRecentUserMessagesTool};
pub(crate) use reply_message::ReplyMessageTool;
pub(crate) use web_search::WebSearchTool;
pub(crate) use zihuan_core::agent::tools::memory_tools::{
    MemoryAgentResources as AgentMemoryToolResources,
};
pub(crate) use zihuan_core::agent::SharedTool;

pub(crate) const DEFAULT_TOOL_WEB_SEARCH: &str = "web_search";
pub(crate) const DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO: &str = "get_agent_public_info";
pub(crate) const DEFAULT_TOOL_GET_FUNCTION_LIST: &str = "get_function_list";
pub(crate) const DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES: &str = "get_recent_group_messages";
pub(crate) const DEFAULT_TOOL_GET_RECENT_USER_MESSAGES: &str = "get_recent_user_messages";
pub(crate) const DEFAULT_TOOL_SEARCH_QQ_MESSAGES: &str = "search_qq_messages";
pub(crate) const DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES: &str = "search_similar_images";
pub(crate) const DEFAULT_TOOL_SAVE_IMAGE: &str = "save_image";
pub(crate) const DEFAULT_TOOL_IMAGE_UNDERSTAND: &str = "image_understand";
pub(crate) const DEFAULT_TOOL_MEMORY_AGENT: &str = "memory_agent";
const AGENT_PUBLIC_NAME: &str = "紫幻zihuan-next";
const AGENT_GITHUB_REPOSITORY: &str = "https://github.com/FredYakumo/zihuan-next";
const AGENT_GIT_COMMIT_ID: &str = "unknown";

pub fn build_info_brain_tools(
    default_tools_enabled: &HashMap<String, bool>,
    web_search_engine_ref: Option<Arc<dyn WebSearchEngine>>,
    rdb_pool: Option<RelationalDbConnection>,
    s3_ref: Option<Arc<S3Ref>>,
    retrieval_store: Option<Arc<RetrievalStoreRef>>,
    local_memory_store: Option<Arc<LocalMemoryStore>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
    llm: Option<Arc<dyn LLMBase>>,
    memory_access: AgentMemoryAccessContext,
    current_message: String,
) -> Vec<Box<dyn Tool>> {
    fn is_enabled(map: &HashMap<String, bool>, name: &str) -> bool {
        *map.get(name).unwrap_or(&true)
    }

    let mut tools: Vec<Box<dyn Tool>> = Vec::new();
    let dashboard_target = ToolNotificationTarget::dashboard();

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_WEB_SEARCH) {
        if let Some(engine) = web_search_engine_ref.as_ref() {
            tools.push(Box::new(WebSearchTool::new(engine.clone())));
        }
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO) {
        tools.push(Box::new(GetAgentPublicInfoTool::new(current_message)));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_FUNCTION_LIST) {
        tools.push(Box::new(GetFunctionListTool));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES) {
        tools.push(Box::new(GetRecentGroupMessagesTool::new(
            rdb_pool.clone(),
            dashboard_target.clone(),
        )));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_RECENT_USER_MESSAGES) {
        tools.push(Box::new(GetRecentUserMessagesTool::new(
            rdb_pool.clone(),
            dashboard_target.clone(),
        )));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES) {
        if let Some(engine) = web_search_engine_ref {
            tools.push(Box::new(SearchSimilarImagesTool::new(
                retrieval_store.clone(),
                embedding_model.clone(),
                engine,
                None,
                dashboard_target.clone(),
            )));
        }
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_SAVE_IMAGE) {
        if s3_ref.is_some() && retrieval_store.is_some() && embedding_model.is_some() {
            tools.push(Box::new(SaveImageTool::new(
                retrieval_store.clone(),
                embedding_model.clone(),
                s3_ref.clone(),
                rdb_pool.clone(),
            )));
        }
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_IMAGE_UNDERSTAND) {
        tools.push(Box::new(ImageUnderstandTool::new(None, rdb_pool, s3_ref, dashboard_target)));
    }

    if let Some(llm) = llm {
        if let Some(memory) =
            MemoryCapability::resolve(local_memory_store, retrieval_store, embedding_model)
        {
            let service_context = AgentServiceContext::new()
                .with_memory(Some(memory))
                .with_llm(LLM_KIND_MAIN, llm);
            let mut host = service_context.build_host(memory_access);
            if is_enabled(default_tools_enabled, DEFAULT_TOOL_MEMORY_AGENT) {
                if let Some(tool) = host.tool(DEFAULT_TOOL_MEMORY_AGENT) {
                    tools.push(Box::new(SharedTool::new(tool)));
                }
            }
        }
    }

    tools
}

/// Memory access context for one QQ turn: memories are scoped to the sending user and, in
/// group chats, to the conversation; private chats fall back to the event's originating group.
pub(crate) fn qq_memory_access_context(
    sender_id: &str,
    target_id: &str,
    is_group: bool,
    event_group_id: Option<i64>,
) -> AgentMemoryAccessContext {
    AgentMemoryAccessContext {
        sender_id: Some(sender_id.to_string()),
        group_id: if is_group {
            Some(target_id.to_string())
        } else {
            event_group_id.map(|value| value.to_string())
        },
        is_group,
        admin: false,
        skip_expiry_extend: false,
    }
}

/// Resolves the memory backend job scripts share with the agent's memory tools: the local
/// on-disk store when configured, otherwise the external retrieval store. `None` when the
/// agent has no memory backend at all.
pub(crate) fn build_job_memory_resources(
    local_memory_store: Option<Arc<LocalMemoryStore>>,
    retrieval_store: Option<Arc<RetrievalStoreRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
) -> Option<JobMemoryResources> {
    MemoryCapability::resolve(local_memory_store, retrieval_store, embedding_model).map(
        |memory| JobMemoryResources {
            memory_backend: memory.backend,
            embedding_model: memory.embedding_model,
        },
    )
}
pub(crate) fn format_public_info_message(message: &str) -> serde_json::Value {
    serde_json::json!({
        "agent_name": AGENT_PUBLIC_NAME,
        "github_repository": AGENT_GITHUB_REPOSITORY,
        "git_commit_id": AGENT_GIT_COMMIT_ID,
        "message": message,
    })
}
