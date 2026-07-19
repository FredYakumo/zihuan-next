use std::env;
use std::sync::Arc;

use chrono::Datelike;
use zihuan_core::error::Result;
use zihuan_core::runtime::block_async;
use zihuan_core::url_utils::{image_content_type_from_bytes, supported_image_content_type};
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
                region: env::var("OBJECT_STORAGE_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
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

    pub async fn object_url_for_key(&self, key: &str) -> Result<String> {
        self.inner.object_url_for_key(key).await
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
    let object_url = object_storage.put_object(&key, &input.content_type, &input.bytes).await?;

    Ok(SavedImageObject { object_key: key, object_url })
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

pub fn upload_remote_image_to_s3(s3_ref: &S3Ref, url: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let resp = client.get(url).send()?;
    if !resp.status().is_success() {
        return Err(zihuan_core::error::Error::StringError(format!(
            "image download returned status {}",
            resp.status()
        )));
    }
    let response_content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = resp.bytes()?.to_vec();
    let content_type = verified_image_content_type(response_content_type.as_deref(), &bytes)?;
    let key = zihuan_core::utils::string_utils::derive_tavily_s3_key(url);
    let s3_ref_clone = s3_ref.clone();
    block_async(async move {
        s3_ref_clone.put_object(&key, content_type, &bytes).await?;
        Ok(key)
    })
}

fn verified_image_content_type(response_content_type: Option<&str>, bytes: &[u8]) -> Result<&'static str> {
    if bytes.is_empty() {
        return Err(zihuan_core::error::Error::StringError(
            "image download returned an empty body".to_string(),
        ));
    }

    let response_content_type = response_content_type.and_then(supported_image_content_type).ok_or_else(|| {
        zihuan_core::error::Error::StringError("image download returned no supported image content type".to_string())
    })?;
    let detected_content_type = image_content_type_from_bytes(bytes).ok_or_else(|| {
        zihuan_core::error::Error::StringError(
            "image download body does not match a supported image signature".to_string(),
        )
    })?;

    if response_content_type != detected_content_type {
        return Err(zihuan_core::error::Error::StringError(format!(
            "image download content type mismatch: response={}, detected={}",
            response_content_type, detected_content_type
        )));
    }

    Ok(detected_content_type)
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
