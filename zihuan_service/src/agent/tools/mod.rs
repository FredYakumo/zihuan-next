use std::collections::HashMap;
use std::sync::Arc;

use zihuan_agent::brain::BrainTool;
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::rag::WebSearchEngineRef;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::object_storage::S3Ref;

mod common;
mod editable_qq_agent_tool;
mod image_understand;
mod image_search;
mod info_tools;
mod recent_messages;
mod web_search;

mod build_metadata {
    include!(concat!(env!("OUT_DIR"), "/build_metadata.rs"));
}

pub(crate) use common::{ToolNotificationTarget, QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS};
pub(crate) use editable_qq_agent_tool::EditableQqAgentTool;
pub(crate) use image_understand::{execute_image_understand_tool, ImageUnderstandBrainTool};
pub(crate) use image_search::SearchSimilarImagesBrainTool;
pub(crate) use info_tools::{GetAgentPublicInfoBrainTool, GetFunctionListBrainTool};
pub(crate) use recent_messages::{GetRecentGroupMessagesBrainTool, GetRecentUserMessagesBrainTool};
pub(crate) use web_search::WebSearchBrainTool;

pub(crate) const DEFAULT_TOOL_WEB_SEARCH: &str = "web_search";
pub(crate) const DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO: &str = "get_agent_public_info";
pub(crate) const DEFAULT_TOOL_GET_FUNCTION_LIST: &str = "get_function_list";
pub(crate) const DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES: &str = "get_recent_group_messages";
pub(crate) const DEFAULT_TOOL_GET_RECENT_USER_MESSAGES: &str = "get_recent_user_messages";
pub(crate) const DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES: &str = "search_similar_images";
pub(crate) const DEFAULT_TOOL_IMAGE_UNDERSTAND: &str = "image_understand";
const AGENT_PUBLIC_NAME: &str = "紫幻zihuan-next";
const AGENT_GITHUB_REPOSITORY: &str = "https://github.com/FredYakumo/zihuan-next";
const AGENT_GIT_COMMIT_ID: &str = build_metadata::ZIHUAN_GIT_COMMIT_ID;

pub(crate) fn build_info_brain_tools(
    default_tools_enabled: &HashMap<String, bool>,
    web_search_engine_ref: Option<Arc<WebSearchEngineRef>>,
    mysql_ref: Option<Arc<MySqlConfig>>,
    s3_ref: Option<Arc<S3Ref>>,
    weaviate_image_ref: Option<Arc<WeaviateRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
    current_message: String,
) -> Vec<Box<dyn BrainTool>> {
    fn is_enabled(map: &HashMap<String, bool>, name: &str) -> bool {
        *map.get(name).unwrap_or(&true)
    }

    let mut tools: Vec<Box<dyn BrainTool>> = Vec::new();
    let dashboard_target = ToolNotificationTarget::dashboard();

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_WEB_SEARCH) {
        if let Some(engine) = web_search_engine_ref.as_ref() {
            tools.push(Box::new(WebSearchBrainTool::new(
                engine.clone(),
                dashboard_target.clone(),
            )));
        }
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO) {
        tools.push(Box::new(GetAgentPublicInfoBrainTool::new(current_message)));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_FUNCTION_LIST) {
        tools.push(Box::new(GetFunctionListBrainTool));
    }

    if is_enabled(
        default_tools_enabled,
        DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES,
    ) {
        tools.push(Box::new(GetRecentGroupMessagesBrainTool::new(
            mysql_ref.clone(),
            dashboard_target.clone(),
        )));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_GET_RECENT_USER_MESSAGES) {
        tools.push(Box::new(GetRecentUserMessagesBrainTool::new(
            mysql_ref.clone(),
            dashboard_target.clone(),
        )));
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES) {
        if let Some(engine) = web_search_engine_ref {
            tools.push(Box::new(SearchSimilarImagesBrainTool::new(
                weaviate_image_ref,
                embedding_model,
                engine,
                None,
                dashboard_target.clone(),
            )));
        }
    }

    if is_enabled(default_tools_enabled, DEFAULT_TOOL_IMAGE_UNDERSTAND) {
        tools.push(Box::new(ImageUnderstandBrainTool::new(
            None,
            mysql_ref,
            s3_ref,
            dashboard_target,
        )));
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
