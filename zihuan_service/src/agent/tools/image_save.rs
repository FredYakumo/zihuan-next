use std::sync::Arc;

use log::{info, warn};
use serde_json::Value;

use ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};
use storage_handler::{upload_remote_image_to_s3, upsert_image_record};
use zihuan_agent::brain::BrainTool;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::url_utils::content_type_from_url;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::object_storage::S3Ref;

use super::common::{optional_string_argument, StaticFunctionToolSpec};

const LOG_PREFIX: &str = "[QqChatAgentService]";

pub(crate) struct SaveImageBrainTool {
    weaviate_image_ref: Option<Arc<WeaviateRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
    s3_ref: Option<Arc<S3Ref>>,
}

impl SaveImageBrainTool {
    pub(crate) fn new(
        weaviate_image_ref: Option<Arc<WeaviateRef>>,
        embedding_model: Option<Arc<dyn EmbeddingBase>>,
        s3_ref: Option<Arc<S3Ref>>,
    ) -> Self {
        Self {
            weaviate_image_ref,
            embedding_model,
            s3_ref,
        }
    }
}

impl BrainTool for SaveImageBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "save_image",
            description: "将一张图片保存到图片库，以便后续通过 search_similar_images 检索和发送。传入图片的 URL 即可完成下载、存储和向量化索引。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "image_url": { "type": "string", "description": "要保存的图片 URL" },
                    "name": { "type": "string", "description": "可选：图片名称" },
                    "description": { "type": "string", "description": "可选：图片描述，用于语义检索" }
                },
                "required": ["image_url"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let image_url = optional_string_argument(arguments, "image_url")
                .ok_or_else(|| Error::ValidationError("image_url is required".to_string()))?;

            let name = optional_string_argument(arguments, "name");
            let description = optional_string_argument(arguments, "description");

            let s3_ref = self.s3_ref.as_ref().ok_or_else(|| {
                Error::ValidationError("save_image requires RustFS (S3) storage to be configured".to_string())
            })?;

            let rustfs_path = upload_remote_image_to_s3(s3_ref, &image_url).map_err(|err| {
                warn!(
                    "{LOG_PREFIX} save_image failed to download/upload image {} into RustFS: {}",
                    image_url, err
                );
                err
            })?;

            let media = PersistedMedia::new(
                PersistedMediaSource::AgentSave,
                image_url.clone(),
                rustfs_path.clone(),
                name,
                description.clone(),
                Some(content_type_from_url(&image_url).to_string()),
            );

            if let (Some(weaviate_image_ref), Some(embedding_model)) =
                (self.weaviate_image_ref.as_ref(), self.embedding_model.as_ref())
            {
                let embedding_text = description.as_deref().unwrap_or(&image_url);
                let description_vector = embedding_model
                    .inference(embedding_text)
                    .unwrap_or_else(|_| embedding_model.inference(&image_url).unwrap_or_default());

                if !description_vector.is_empty() {
                    if let Err(err) = upsert_image_record(weaviate_image_ref, &media, &description_vector, None) {
                        warn!("{LOG_PREFIX} save_image failed to persist image record into Weaviate: {}", err);
                    }
                } else {
                    info!(
                        "{LOG_PREFIX} save_image skipped Weaviate upsert for image_url='{}' because embedding vector is empty",
                        image_url
                    );
                }
            }

            info!(
                "{LOG_PREFIX} save_image saved image_url='{}' -> rustfs_path='{}', media_id='{}'",
                image_url, rustfs_path, media.media_id
            );

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
