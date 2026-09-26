use std::sync::Arc;

use log::{info, warn};
use serde_json::Value;

use zihuan_core::agent::tools::Tool;
use zihuan_core::error::{Error, Result};
use zihuan_core::graph::object_storage::S3Ref;
use zihuan_core::ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};
use zihuan_core::model_inference::llm::embedding_base::EmbeddingBase;
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::rag::{WebSearchEngine, WebSearchImage};
use zihuan_core::retrieval::RetrievalStoreRef;
use zihuan_core::storage::{
    persist_media_to_store, search_images_in_store, upload_remote_image_to_s3,
};

use super::common::{
    extract_string_field, optional_bool_argument, optional_string_argument,
    sanitize_positive_limit, StaticFunctionToolSpec, ToolNotificationTarget,
};
use zihuan_core::url_utils::content_type_from_url;

const LOG_PREFIX: &str = "[QqChatAgentService]";
const DEFAULT_SEMANTIC_SEARCH_LIMIT: i64 = 5;
const MAX_SEMANTIC_SEARCH_LIMIT: i64 = 20;
const WEAVIATE_IMAGE_MAX_GOOD_DISTANCE: f64 = 0.55;

pub(crate) struct SearchSimilarImagesTool {
    retrieval_store: Option<Arc<RetrievalStoreRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
    web_search_engine_ref: Arc<dyn WebSearchEngine>,
    s3_ref: Option<Arc<S3Ref>>,
}

impl SearchSimilarImagesTool {
    pub(crate) fn new(
        retrieval_store: Option<Arc<RetrievalStoreRef>>,
        embedding_model: Option<Arc<dyn EmbeddingBase>>,
        web_search_engine_ref: Arc<dyn WebSearchEngine>,
        s3_ref: Option<Arc<S3Ref>>,
        _notification_target: ToolNotificationTarget,
    ) -> Self {
        Self {
            retrieval_store,
            embedding_model,
            web_search_engine_ref,
            s3_ref,
        }
    }
}

impl Tool for SearchSimilarImagesTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "search_similar_images",
            description: "按语义搜索可发送的相关图片，返回可直接发送的图片候选（含 media_id 与来源信息）。当默认结果不理想时，可通过 force_web_search 扩大检索范围。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "要搜索的图片语义查询文本" },
                    "limit": { "type": "integer", "description": "返回数量，默认 5，最大 20" },
                    "force_web_search": { "type": "boolean", "description": "可选：当默认结果不理想时设为 true，用于扩大检索范围并获取更多候选" }
                },
                "required": ["query"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let query = optional_string_argument(arguments, "query")
                .ok_or_else(|| Error::ValidationError("query is required".to_string()))?;
            let limit = sanitize_positive_limit(
                arguments.get("limit").and_then(Value::as_i64),
                DEFAULT_SEMANTIC_SEARCH_LIMIT,
                MAX_SEMANTIC_SEARCH_LIMIT,
            );
            let force_web_search =
                optional_bool_argument(arguments, "force_web_search").unwrap_or(false);

            if !force_web_search {
                if let (Some(retrieval_store), Some(embedding_model)) =
                    (self.retrieval_store.as_ref(), self.embedding_model.as_ref())
                {
                    let payload = search_images_in_store(
                        retrieval_store,
                        embedding_model.as_ref(),
                        &query,
                        limit as usize,
                        Some(WEAVIATE_IMAGE_MAX_GOOD_DISTANCE),
                        None,
                    )?;
                    let mut items = payload
                        .get("images")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    items.retain(|item| {
                        extract_string_field(item, "rustfs_path")
                            .map(|value| !value.trim().is_empty())
                            .unwrap_or(false)
                    });
                    if let Some(s3) = self.s3_ref.as_ref() {
                        let local_base = s3_local_base(s3);
                        items.retain(|item| {
                            extract_string_field(item, "rustfs_path")
                                .as_deref()
                                .map(|p| is_local_s3_path(p, &local_base))
                                .unwrap_or(false)
                        });
                    }

                    if !items.is_empty() {
                        return Ok(serde_json::json!({
                            "ok": true,
                            "source": retrieval_store.backend().as_str(),
                            "images": format_image_lookup_results(&items),
                        }));
                    }
                }
            } else {
                info!(
                    "{LOG_PREFIX} search_similar_images skipping retrieval store and forcing Tavily web search for query='{}'",
                    query
                );
            }

            let fallback_count = limit.min(10) as i64;
            let web_images: Vec<WebSearchImage> = self
                .web_search_engine_ref
                .search_images(&format!("{} 图片", query), fallback_count)?;

            let Some(s3_ref) = self.s3_ref.as_ref() else {
                return Err(Error::ValidationError(
                    "search_similar_images requires RustFS before returning image send candidates"
                        .to_string(),
                ));
            };

            let mut stored_images = Vec::new();
            for image in &web_images {
                let description = image.description.as_deref().unwrap_or(&image.url);
                let rustfs_path = match upload_remote_image_to_s3(s3_ref, &image.url) {
                    Ok(path) => path,
                    Err(err) => {
                        warn!(
                            "{LOG_PREFIX} Failed to download/upload web search image {} into RustFS: {}",
                            image.url, err
                        );
                        continue;
                    }
                };
                let media = PersistedMedia::new(
                    PersistedMediaSource::WebSearch,
                    image.url.clone(),
                    rustfs_path.clone(),
                    None,
                    image.description.clone(),
                    Some(content_type_from_url(&image.url).to_string()),
                );

                stored_images.push(serde_json::json!({
                    "media_id": media.media_id,
                    "original_source": media.original_source,
                    "rustfs_path": media.rustfs_path,
                    "description": media.description,
                    "mime_type": media.mime_type,
                    "source": media.source.to_string(),
                }));

                if let (Some(retrieval_store), Some(embedding_model)) =
                    (self.retrieval_store.as_ref(), self.embedding_model.as_ref())
                {
                    let description_vector = embedding_model
                        .inference(description)
                        .unwrap_or_else(|_| embedding_model.inference(&query).unwrap_or_default());
                    if !description_vector.is_empty() {
                        if let Err(err) = persist_media_to_store(
                            retrieval_store,
                            &media,
                            &description_vector,
                            None,
                        ) {
                            warn!(
                                "{LOG_PREFIX} Failed to persist web search image fallback result into retrieval store: {}",
                                err
                            );
                        }
                    }
                }
            }

            Ok(serde_json::json!({
                "ok": true,
                "source": "tavily",
                "images": stored_images,
            }))
        })();

        match result {
            Ok(value) => value.to_string(),
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }
}

fn extract_distance(value: &Value) -> Option<f64> {
    value
        .get("_additional")
        .and_then(|extra| extra.get("distance"))
        .and_then(Value::as_f64)
}

fn format_image_lookup_results(items: &[Value]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "media_id": extract_string_field(item, "media_id"),
                    "original_source": extract_string_field(item, "original_source"),
                    "rustfs_path": extract_string_field(item, "rustfs_path"),
                    "name": extract_string_field(item, "name"),
                    "description": extract_string_field(item, "description"),
                    "mime_type": extract_string_field(item, "mime_type"),
                    "source": extract_string_field(item, "source"),
                    "distance": extract_distance(item),
                })
            })
            .collect(),
    )
}

fn s3_local_base(s3_ref: &S3Ref) -> String {
    if let Some(ref pub_base) = s3_ref.public_base_url {
        pub_base.trim_end_matches('/').to_string()
    } else if s3_ref.path_style {
        format!("{}/{}", s3_ref.endpoint.trim_end_matches('/'), s3_ref.bucket.trim_matches('/'))
    } else {
        s3_ref.endpoint.trim_end_matches('/').to_string()
    }
}

fn is_local_s3_path(path: &str, local_base: &str) -> bool {
    !(path.starts_with("http://") || path.starts_with("https://")) || path.starts_with(local_base)
}
