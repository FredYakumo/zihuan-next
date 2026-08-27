use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::memory_agent::{
    MemoryAgentResources, MemoryBackend, MemoryBrainAgent, MemoryBrainAgentContextTool, MemoryBrainAgentTool,
};
use zihuan_core::storage::AgentMemoryAccessContext;
use zihuan_core::storage::ElasticsearchRef;
use zihuan_core::storage::LocalMemoryStore;
use zihuan_core::agent::tools::Tool;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::rag::WebSearchEngine;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_core::graph::object_storage::S3Ref;

mod agent_state;
mod common;
mod deep_research;
mod editable_qq_agent_tool;
mod image_save;
mod image_search;
mod image_understand;
mod info_tools;
mod natural_language_reply;
mod recent_messages;
mod reply_message;
mod research;
mod web_search;

pub(crate) use zihuan_core::memory_agent::{MemoryAgentResources as AgentMemoryToolResources, MemoryBackend as AgentMemoryBackend};
pub(crate) use agent_state::UpdateAgentStateTool;
pub(crate) use common::{ToolNotificationTarget, QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS};
pub(crate) use deep_research::RunDeepResearchSubagentTool;
pub(crate) use editable_qq_agent_tool::EditableQqAgentTool;
pub(crate) use image_save::SaveImageTool;
pub(crate) use image_search::SearchSimilarImagesTool;
pub(crate) use image_understand::{execute_image_understand_tool, ImageUnderstandTool};
pub(crate) use info_tools::{GetAgentPublicInfoTool, GetFunctionListTool};
pub(crate) use natural_language_reply::{
    AfterBrainAgent, ModelIdentityContext, QqReplyReviewRequest, QqReplyReviewResult,
};
pub(crate) use recent_messages::{GetRecentGroupMessagesTool, GetRecentUserMessagesTool};
pub(crate) use reply_message::ReplyMessageTool;
pub(crate) use research::RunResearchSubagentTool;
pub(crate) use web_search::WebSearchTool;

pub(crate) const DEFAULT_TOOL_WEB_SEARCH: &str = "web_search";
pub(crate) const DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO: &str = "get_agent_public_info";
pub(crate) const DEFAULT_TOOL_GET_FUNCTION_LIST: &str = "get_function_list";
pub(crate) const DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES: &str = "get_recent_group_messages";
pub(crate) const DEFAULT_TOOL_GET_RECENT_USER_MESSAGES: &str = "get_recent_user_messages";
pub(crate) const DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES: &str = "search_similar_images";
pub(crate) const DEFAULT_TOOL_SAVE_IMAGE: &str = "save_image";
pub(crate) const DEFAULT_TOOL_IMAGE_UNDERSTAND: &str = "image_understand";
pub(crate) const DEFAULT_TOOL_MEMORY_AGENT: &str = "memory_agent";
pub(crate) const DEFAULT_TOOL_MEMORY_AGENT_WITH_CONTEXT: &str = "memory_agent_with_context";
const AGENT_PUBLIC_NAME: &str = "紫幻zihuan-next";
const AGENT_GITHUB_REPOSITORY: &str = "https://github.com/FredYakumo/zihuan-next";
const AGENT_GIT_COMMIT_ID: &str = "unknown";

pub fn build_info_brain_tools(
    default_tools_enabled: &HashMap<String, bool>,
    web_search_engine_ref: Option<Arc<dyn WebSearchEngine>>,
    rdb_pool: Option<RelationalDbConnection>,
    s3_ref: Option<Arc<S3Ref>>,
    weaviate_image_ref: Option<Arc<WeaviateRef>>,
    elasticsearch_image_ref: Option<Arc<ElasticsearchRef>>,
    weaviate_memory_ref: Option<Arc<WeaviateRef>>,
    elasticsearch_memory_ref: Option<Arc<ElasticsearchRef>>,
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
                weaviate_image_ref.clone(),
                embedding_model.clone(),
                engine,
                None,
                dashboard_target.clone(),
            )));
        }
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_SAVE_IMAGE) {
        if s3_ref.is_some()
            && (weaviate_image_ref.is_some() || elasticsearch_image_ref.is_some())
            && embedding_model.is_some()
        {
            tools.push(Box::new(SaveImageTool::new(
                weaviate_image_ref.clone(),
                elasticsearch_image_ref.clone(),
                embedding_model.clone(),
                s3_ref.clone(),
                rdb_pool.clone(),
            )));
        }
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_IMAGE_UNDERSTAND) {
        tools.push(Box::new(ImageUnderstandTool::new(
            None,
            rdb_pool,
            s3_ref,
            dashboard_target,
        )));
    }

    let memory_backend = local_memory_store
        .map(MemoryBackend::LocalFile)
        .or_else(|| elasticsearch_memory_ref
        .map(MemoryBackend::Elasticsearch)
        .or_else(|| weaviate_memory_ref.map(MemoryBackend::Weaviate)));
    if let (Some(memory_backend), Some(llm)) = (memory_backend, llm) {
        let memory_resources = MemoryAgentResources {
            memory_backend,
            embedding_model,
            llm,
            access: memory_access,
        };
        let memory_agent = MemoryBrainAgent::new(memory_resources);
        if is_enabled(default_tools_enabled, DEFAULT_TOOL_MEMORY_AGENT) {
            tools.push(Box::new(MemoryBrainAgentTool::new(memory_agent.clone())));
        }
        if is_enabled(default_tools_enabled, DEFAULT_TOOL_MEMORY_AGENT_WITH_CONTEXT) {
            tools.push(Box::new(MemoryBrainAgentContextTool::new(memory_agent)));
        }
    }

    tools
}
pub(crate) fn format_public_info_message(message: &str) -> serde_json::Value {
    serde_json::json!({
        "agent_name": AGENT_PUBLIC_NAME,
        "github_repository": AGENT_GITHUB_REPOSITORY,
        "git_commit_id": AGENT_GIT_COMMIT_ID,
        "message": message,
    })
}
