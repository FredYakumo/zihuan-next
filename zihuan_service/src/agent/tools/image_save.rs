use std::sync::Arc;

use log::{info, warn};
use serde_json::Value;

use ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};
use storage_handler::{upload_remote_image_to_s3, upsert_image_record};
use zihuan_agent::brain::BrainTool;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::url_utils::content_type_from_url;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::message_restore::{persist_media_to_record, query_media_by_id};
use zihuan_graph_engine::object_storage::S3Ref;

use super::common::{optional_string_argument, StaticFunctionToolSpec};

const LOG_PREFIX: &str = "[QqChatAgentService]";

pub(crate) struct SaveImageBrainTool {
    weaviate_image_ref: Option<Arc<WeaviateRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
    s3_ref: Option<Arc<S3Ref>>,
    rdb_pool: Option<RelationalDbConnection>,
}

impl SaveImageBrainTool {
    pub(crate) fn new(
        weaviate_image_ref: Option<Arc<WeaviateRef>>,
        embedding_model: Option<Arc<dyn EmbeddingBase>>,
        s3_ref: Option<Arc<S3Ref>>,
        rdb_pool: Option<RelationalDbConnection>,
    ) -> Self {
        Self {
            weaviate_image_ref,
            embedding_model,
            s3_ref,
            rdb_pool,
        }
    }
}

impl BrainTool for SaveImageBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "save_image",
            description: "将一张图片保存到图片库，以便后续通过 search_similar_images 检索和发送。提供 image_url（外部图片 URL）或 media_id（聊天中已有的图片 ID）即可完成下载、存储和向量化索引。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "image_url": { "type": "string", "description": "要保存的外部图片 URL。与 media_id 二选一" },
                    "media_id": { "type": "string", "description": "聊天中已有图片的 media_id（格式如 media-xxx）。与 image_url 二选一" },
                    "name": { "type": "string", "description": "可选：图片名称" },
                    "description": { "type": "string", "description": "可选：图片描述，用于语义检索" }
                },
                "required": []
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let image_url = optional_string_argument(arguments, "image_url");
            let media_id = optional_string_argument(arguments, "media_id");

            let resolved_url = match (&image_url, &media_id) {
                (Some(url), _) => url.clone(),
                (None, Some(media_id)) => {
                    let media = query_media_by_id(
                        media_id,
                        self.rdb_pool.as_ref(),
                    )?
                    .ok_or_else(|| {
                        Error::ValidationError(format!("save_image could not find media_id '{}'", media_id))
                    })?;
                    if media.original_source.trim().is_empty() {
                        return Err(Error::ValidationError(format!(
                            "save_image: media_id '{}' has no usable source URL",
                            media_id
                        )));
                    }
                    media.original_source
                }
                (None, None) => {
                    return Err(Error::ValidationError(
                        "save_image requires either image_url or media_id".to_string(),
                    ));
                }
            };

            let name = optional_string_argument(arguments, "name");
            let description = optional_string_argument(arguments, "description");

            let s3_ref = self.s3_ref.as_ref().ok_or_else(|| {
                Error::ValidationError("save_image requires RustFS (S3) storage to be configured".to_string())
            })?;

            let rustfs_path = upload_remote_image_to_s3(s3_ref, &resolved_url).map_err(|err| {
                warn!(
                    "{LOG_PREFIX} save_image failed to download/upload image {} into RustFS: {}",
                    resolved_url, err
                );
                err
            })?;

            let media = PersistedMedia::new(
                PersistedMediaSource::AgentSave,
                resolved_url.clone(),
                rustfs_path.clone(),
                name,
                description.clone(),
                Some(content_type_from_url(&resolved_url).to_string()),
            );

            if let (Some(weaviate_image_ref), Some(embedding_model)) =
                (self.weaviate_image_ref.as_ref(), self.embedding_model.as_ref())
            {
                let embedding_text = description.as_deref().unwrap_or(&resolved_url);
                let description_vector = embedding_model
                    .inference(embedding_text)
                    .unwrap_or_else(|_| embedding_model.inference(&resolved_url).unwrap_or_default());

                if !description_vector.is_empty() {
                    if let Err(err) = upsert_image_record(weaviate_image_ref, &media, &description_vector, None) {
                        warn!("{LOG_PREFIX} save_image failed to persist image record into Weaviate: {}", err);
                    }
                } else {
                    info!(
                        "{LOG_PREFIX} save_image skipped Weaviate upsert for image_url='{}' because embedding vector is empty",
                        resolved_url
                    );
                }
            }

            info!(
                "{LOG_PREFIX} save_image saved image_url='{}' -> rustfs_path='{}', media_id='{}'",
                resolved_url, rustfs_path, media.media_id
            );

            if let Some(rdb_pool) = &self.rdb_pool {
                if let Err(err) = persist_media_to_record(rdb_pool, &media) {
                    warn!("{LOG_PREFIX} save_image failed to persist media to media_record: {}", err);
                }
            }

            Ok(serde_json::json!({
                "ok": true,
                "media_id": media.media_id,
                "original_source": media.original_source,
                "rustfs_path": media.rustfs_path,
                "name": media.name,
                "description": media.description,
                "mime_type": media.mime_type,
                "source": media.source.to_string(),
            }))
        })();

        match result {
            Ok(value) => value.to_string(),
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }
}
