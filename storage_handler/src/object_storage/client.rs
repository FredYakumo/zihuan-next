use std::env;
use std::sync::Arc;

use chrono::Datelike;
use zihuan_core::error::Result;
use zihuan_graph_engine::object_storage::S3Ref;

#[derive(Debug, Clone)]
pub struct ObjectStorageConfig {
    inner: Arc<S3Ref>,
}

#[derive(Debug, Clone)]
pub struct ImageObjectStorageInput {
    pub message_id: i64,
    pub segment_index: usize,
    pub file_name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct SavedImageObject {
    pub object_key: String,
    pub object_url: String,
}

impl ObjectStorageConfig {
    pub fn from_env() -> Option<Self> {
        let endpoint = env::var("OBJECT_STORAGE_ENDPOINT").ok()?;
        let bucket = env::var("OBJECT_STORAGE_BUCKET").ok()?;
        let access_key = env::var("OBJECT_STORAGE_ACCESS_KEY").ok()?;
        let secret_key = env::var("OBJECT_STORAGE_SECRET_KEY").ok()?;

        Some(Self {
            inner: Arc::new(S3Ref {
                endpoint,
                bucket,
                access_key,
                secret_key,
                region: env::var("OBJECT_STORAGE_REGION")
                    .unwrap_or_else(|_| "us-east-1".to_string()),
                public_base_url: env::var("OBJECT_STORAGE_PUBLIC_BASE_URL").ok(),
                path_style: env::var("OBJECT_STORAGE_PATH_STYLE")
                    .ok()
                    .map(|value| !matches!(value.as_str(), "0" | "false" | "FALSE"))
                    .unwrap_or(true),
            }),
        })
    }

    pub fn into_inner(self) -> Arc<S3Ref> {
        self.inner
    }

    pub fn as_ref(&self) -> &S3Ref {
        &self.inner
    }

    pub fn object_url_for_key(&self, key: &str) -> Result<String> {
        self.inner.object_url_for_key(key)
    }

    pub async fn put_object(&self, key: &str, content_type: &str, body: &[u8]) -> Result<String> {
        self.inner.put_object(key, content_type, body).await
    }
}

pub async fn save_image_to_object_storage(
    object_storage: &S3Ref,
    input: &ImageObjectStorageInput,
) -> Result<SavedImageObject> {
    let key = build_object_key(input.message_id, input.segment_index, &input.file_name);
    let object_url = object_storage
        .put_object(&key, &input.content_type, &input.bytes)
        .await?;

    Ok(SavedImageObject {
        object_key: key,
        object_url,
    })
}

fn build_object_key(message_id: i64, segment_index: usize, file_name: &str) -> String {
    let now = chrono::Utc::now();
    let safe_file_name = sanitize_filename(file_name);
    format!(
        "qq-images/{}/{:02}/{:02}/{}_{}_{}",
        now.format("%Y"),
        now.month(),
        now.day(),
        message_id,
        segment_index,
        safe_file_name
    )
}

fn sanitize_filename(file_name: &str) -> String {
    let sanitized: String = file_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "image.bin".to_string()
    } else {
        sanitized
    }
}
