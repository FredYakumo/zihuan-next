use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::memory_agent::{
    MemoryAgentResources, MemoryBackend, MemoryBrainAgent, MemoryBrainAgentContextTool, MemoryBrainAgentTool,
};
use zihuan_core::storage::AgentMemoryAccessContext;
use zihuan_core::storage::ElasticsearchRef;
use zihuan_core::agent_runtime::brain::BrainTool;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::rag::WebSearchEngineRef;
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
pub(crate) use agent_state::UpdateAgentStateBrainTool;
pub(crate) use common::{ToolNotificationTarget, QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS};
pub(crate) use deep_research::RunDeepResearchSubagentBrainTool;
pub(crate) use editable_qq_agent_tool::EditableQqAgentTool;
pub(crate) use image_save::SaveImageBrainTool;
pub(crate) use image_search::SearchSimilarImagesBrainTool;
pub(crate) use image_understand::{execute_image_understand_tool, ImageUnderstandBrainTool};
pub(crate) use info_tools::{GetAgentPublicInfoBrainTool, GetFunctionListBrainTool};
pub(crate) use natural_language_reply::{
    review_and_rewrite_reply, ModelIdentityContext, QqReplyReviewRequest, QqReplyReviewResult,
};
pub(crate) use recent_messages::{GetRecentGroupMessagesBrainTool, GetRecentUserMessagesBrainTool};
pub(crate) use reply_message::ReplyMessageBrainTool;
pub(crate) use research::RunResearchSubagentBrainTool;
pub(crate) use web_search::WebSearchBrainTool;

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
    web_search_engine_ref: Option<Arc<WebSearchEngineRef>>,
    rdb_pool: Option<RelationalDbConnection>,
    s3_ref: Option<Arc<S3Ref>>,
    weaviate_image_ref: Option<Arc<WeaviateRef>>,
    elasticsearch_image_ref: Option<Arc<ElasticsearchRef>>,
    weaviate_memory_ref: Option<Arc<WeaviateRef>>,
    elasticsearch_memory_ref: Option<Arc<ElasticsearchRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
    llm: Option<Arc<dyn LLMBase>>,
    memory_access: AgentMemoryAccessContext,
    current_message: String,
) -> Vec<Box<dyn BrainTool>> {
    fn is_enabled(map: &HashMap<String, bool>, name: &str) -> bool {
        *map.get(name).unwrap_or(&true)
    }

    let mut tools: Vec<Box<dyn BrainTool>> = Vec::new();
    let dashboard_target = ToolNotificationTarget::dashboard();

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_WEB_SEARCH) {
        if let Some(engine) = web_search_engine_ref.as_ref() {
            tools.push(Box::new(WebSearchBrainTool::new(engine.clone(), dashboard_target.clone())));
        }
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO) {
        tools.push(Box::new(GetAgentPublicInfoBrainTool::new(current_message)));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_FUNCTION_LIST) {
        tools.push(Box::new(GetFunctionListBrainTool));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES) {
        tools.push(Box::new(GetRecentGroupMessagesBrainTool::new(
            rdb_pool.clone(),
            dashboard_target.clone(),
        )));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_RECENT_USER_MESSAGES) {
        tools.push(Box::new(GetRecentUserMessagesBrainTool::new(
            rdb_pool.clone(),
            dashboard_target.clone(),
        )));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES) {
        if let Some(engine) = web_search_engine_ref {
            tools.push(Box::new(SearchSimilarImagesBrainTool::new(
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
            tools.push(Box::new(SaveImageBrainTool::new(
                weaviate_image_ref.clone(),
                elasticsearch_image_ref.clone(),
                embedding_model.clone(),
                s3_ref.clone(),
                rdb_pool.clone(),
            )));
        }
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_IMAGE_UNDERSTAND) {
        tools.push(Box::new(ImageUnderstandBrainTool::new(
            None,
            rdb_pool,
            s3_ref,
            dashboard_target,
        )));
    }

    let memory_backend = elasticsearch_memory_ref
        .map(MemoryBackend::Elasticsearch)
        .or_else(|| weaviate_memory_ref.map(MemoryBackend::Weaviate));
    if let (Some(memory_backend), Some(embedding_model), Some(llm)) = (memory_backend, embedding_model.clone(), llm) {
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
