use std::cmp::Ordering;
use std::sync::Arc;

use log::{info, warn};
use serde_json::Value;

use ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};
use storage_handler::upsert_image_record;
use zihuan_agent::brain::BrainTool;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::rag::{TavilyImage, TavilyRef};
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::object_storage::S3Ref;

use super::common::{
    content_type_from_url, extract_string_field, optional_bool_argument, optional_string_argument,
    sanitize_positive_limit, upload_remote_image_to_s3, StaticFunctionToolSpec,
    ToolNotificationTarget,
};

const LOG_PREFIX: &str = "[QqChatAgent]";
const DEFAULT_SEMANTIC_SEARCH_LIMIT: i64 = 5;
const MAX_SEMANTIC_SEARCH_LIMIT: i64 = 20;
const WEAVIATE_IMAGE_MAX_GOOD_DISTANCE: f64 = 0.55;

pub(crate) struct SearchSimilarImagesBrainTool {
    weaviate_image_ref: Option<Arc<WeaviateRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
    tavily_ref: Arc<TavilyRef>,
    s3_ref: Option<Arc<S3Ref>>,
    notification_target: ToolNotificationTarget,
}

impl SearchSimilarImagesBrainTool {
    pub(crate) fn new(
        weaviate_image_ref: Option<Arc<WeaviateRef>>,
        embedding_model: Option<Arc<dyn EmbeddingBase>>,
        tavily_ref: Arc<TavilyRef>,
        s3_ref: Option<Arc<S3Ref>>,
        notification_target: ToolNotificationTarget,
    ) -> Self {
        Self {
            weaviate_image_ref,
            embedding_model,
            tavily_ref,
            s3_ref,
            notification_target,
        }
    }
}

impl BrainTool for SearchSimilarImagesBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "search_similar_images",
            description: "搜索图片：默认优先在 Weaviate 图片 collection 做向量检索，找不到合适结果时可设置 force_web_search=true 强制使用 Tavily 联网搜索，并把联网结果回填 Weaviate",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "要搜索的图片语义查询文本" },
                    "limit": { "type": "integer", "description": "返回数量，默认 5，最大 20" },
                    "force_web_search": { "type": "boolean", "description": "是否强制跳过本地 Weaviate 检索，直接使用 Tavily 联网搜索；当本地图片不合适时设为 true" }
                },
                "required": ["query"]
            }),
        })
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.notification_target.notify_progress(call_content);

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
                if let (Some(weaviate_image_ref), Some(embedding_model)) = (
                    self.weaviate_image_ref.as_ref(),
                    self.embedding_model.as_ref(),
                ) {
                    let vector = embedding_model.inference(&query)?;
                    let mut items = run_weaviate_image_get_query(
                        weaviate_image_ref,
                        limit,
                        Some(&vector),
                        None,
                        None,
                        true,
                    )?;
                    items.sort_by(semantic_result_order);
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
                    let candidate_count_after_path_filters = items.len();
                    let dropped_by_distance: Vec<String> = items
                        .iter()
                        .filter(|item| {
                            extract_distance(item)
                                .map(|d| d > WEAVIATE_IMAGE_MAX_GOOD_DISTANCE)
                                .unwrap_or(false)
                        })
                        .map(format_weaviate_image_candidate_for_log)
                        .collect();
                    items.retain(|item| {
                        item.get("distance")
                            .and_then(Value::as_f64)
                            .map(|d| d <= WEAVIATE_IMAGE_MAX_GOOD_DISTANCE)
                            .unwrap_or(true)
                    });
                    if !dropped_by_distance.is_empty() {
                        info!(
                            "{LOG_PREFIX} search_similar_images dropped {} Weaviate candidates after URL/path filtering for query='{}' because distance exceeded {}: {}",
                            dropped_by_distance.len(),
                            query,
                            WEAVIATE_IMAGE_MAX_GOOD_DISTANCE,
                            dropped_by_distance.join(", ")
                        );
                    }
                    if candidate_count_after_path_filters > 0 && items.is_empty() {
                        info!(
                            "{LOG_PREFIX} search_similar_images will fall back to Tavily for query='{}' because no Weaviate candidates remained after distance filtering (threshold={})",
                            query,
                            WEAVIATE_IMAGE_MAX_GOOD_DISTANCE
                        );
                    }

                    if !items.is_empty() {
                        return Ok(serde_json::json!({
                            "ok": true,
                            "source": "weaviate",
                            "images": format_image_lookup_results(&items),
                        }));
                    }
                }
            } else {
                info!(
                    "{LOG_PREFIX} search_similar_images skipping Weaviate and forcing Tavily web search for query='{}'",
                    query
                );
            }

            let fallback_count = limit.min(10) as i64;
            let tavily_images: Vec<TavilyImage> = self
                .tavily_ref
                .search_images(&format!("{} 图片", query), fallback_count)?;

            let Some(s3_ref) = self.s3_ref.as_ref() else {
                return Err(Error::ValidationError(
                    "search_similar_images requires RustFS before returning image send candidates"
                        .to_string(),
                ));
            };

            let mut stored_images = Vec::new();
            for image in &tavily_images {
                let description = image.description.as_deref().unwrap_or(&image.url);
                let rustfs_path = match upload_remote_image_to_s3(s3_ref, &image.url) {
                    Ok(path) => path,
                    Err(err) => {
                        warn!(
                            "{LOG_PREFIX} Failed to download/upload tavily image {} into RustFS: {}",
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

                if let (Some(weaviate_image_ref), Some(embedding_model)) = (
                    self.weaviate_image_ref.as_ref(),
                    self.embedding_model.as_ref(),
                ) {
                    let description_vector = embedding_model
                        .inference(description)
                        .unwrap_or_else(|_| embedding_model.inference(&query).unwrap_or_default());
                    if !description_vector.is_empty() {
                        if let Err(err) = upsert_image_record(
                            weaviate_image_ref,
                            &media,
                            &description_vector,
                            None,
                        ) {
                            warn!(
                                "{LOG_PREFIX} Failed to persist tavily image fallback result into weaviate: {}",
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

fn build_get_query_arguments(
    limit: usize,
    near_vector: Option<&[f32]>,
    where_filter: Option<&str>,
    sort: Option<&str>,
) -> String {
    let mut args = Vec::new();
    if let Some(vector) = near_vector {
        let vector_body = vector
            .iter()
            .map(|value| {
                let mut rendered = value.to_string();
                if !rendered.contains('.') && !rendered.contains('e') && !rendered.contains('E') {
                    rendered.push_str(".0");
                }
                rendered
            })
            .collect::<Vec<_>>()
            .join(", ");
        args.push(format!("nearVector: {{ vector: [{vector_body}] }}"));
    }
    if let Some(where_filter) = where_filter {
        args.push(format!("where: {where_filter}"));
    }
    if let Some(sort) = sort {
        args.push(format!("sort: [{sort}]"));
    }
    args.push(format!("limit: {limit}"));
    format!("({})", args.join(", "))
}

fn extract_distance(value: &Value) -> Option<f64> {
    value
        .get("_additional")
        .and_then(|extra| extra.get("distance"))
        .and_then(Value::as_f64)
}

fn format_weaviate_image_candidate_for_log(value: &Value) -> String {
    let path =
        extract_string_field(value, "rustfs_path").unwrap_or_else(|| "<missing-path>".to_string());
    let distance = extract_distance(value)
        .map(|d| format!("{d:.4}"))
        .unwrap_or_else(|| "none".to_string());
    format!("{path} (distance={distance})")
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

fn run_weaviate_image_get_query(
    weaviate_ref: &WeaviateRef,
    limit: usize,
    near_vector: Option<&[f32]>,
    where_filter: Option<&str>,
    sort: Option<&str>,
    include_distance: bool,
) -> Result<Vec<Value>> {
    let arguments = build_get_query_arguments(limit, near_vector, where_filter, sort);
    let mut fields = vec![
        "media_id",
        "original_source",
        "rustfs_path",
        "name",
        "description",
        "mime_type",
        "source",
    ]
    .join(" ");
    if include_distance {
        fields.push_str(" _additional { id distance }");
    }

    let query = format!(
        "{{ Get {{ {}{} {{ {} }} }} }}",
        weaviate_ref.class_name, arguments, fields
    );
    let response = weaviate_ref.execute_graphql_query(&query)?;
    Ok(response
        .get("data")
        .and_then(|value| value.get("Get"))
        .and_then(|value| value.get(&weaviate_ref.class_name))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn s3_local_base(s3_ref: &S3Ref) -> String {
    if let Some(ref pub_base) = s3_ref.public_base_url {
        pub_base.trim_end_matches('/').to_string()
    } else if s3_ref.path_style {
        format!(
            "{}/{}",
            s3_ref.endpoint.trim_end_matches('/'),
            s3_ref.bucket.trim_matches('/')
        )
    } else {
        s3_ref.endpoint.trim_end_matches('/').to_string()
    }
}

fn is_local_s3_path(path: &str, local_base: &str) -> bool {
    !(path.starts_with("http://") || path.starts_with("https://")) || path.starts_with(local_base)
}

fn semantic_result_order(left: &Value, right: &Value) -> Ordering {
    let left_distance = extract_distance(left).unwrap_or(f64::INFINITY);
    let right_distance = extract_distance(right).unwrap_or(f64::INFINITY);
    match left_distance.total_cmp(&right_distance) {
        Ordering::Equal => {
            extract_string_field(right, "send_time").cmp(&extract_string_field(left, "send_time"))
        }
        other => other,
    }
}
