use std::path::Path;
use std::sync::Arc;

use async_recursion::async_recursion;
use async_trait::async_trait;
use base64::Engine;
use log::{debug, info, warn};
use reqwest::Client;
use serde::Deserialize;
use zihuan_core::error::{Error, Result};
use zihuan_core::ims_bot_adapter::models::event_model::MessageEvent;
use zihuan_core::ims_bot_adapter::models::message::{
    ForwardNodeMessage, ImageMessage, Message, PersistedMedia,
};
use zihuan_graph_engine::object_storage::S3Ref;

use super::{save_image_to_object_storage, ImageObjectStorageInput};

const LOG_PREFIX: &str = "[media_cache]";

#[derive(Debug, Clone)]
pub struct PendingImageUpload {
    pub message_id: i64,
    pub segment_index: usize,
    pub image: ImageMessage,
}

#[async_trait]
pub trait ImageCacheAdapter: Clone + Send + Sync + 'static {
    async fn object_storage(&self) -> Option<Arc<S3Ref>>;
    async fn enqueue_pending_image(&self, pending: PendingImageUpload) -> bool;
    async fn dequeue_pending_image(&self) -> Option<PendingImageUpload>;
    async fn set_retry_task_running(&self, running: bool);
    async fn image_detail_request_context(&self) -> (String, String);
}

#[derive(Debug, Deserialize)]
struct NapCatImageDetailResponse {
    data: Option<NapCatImageDetailData>,
}

#[derive(Debug, Deserialize)]
struct NapCatImageDetailData {
    file: Option<String>,
    url: Option<String>,
    file_name: Option<String>,
    base64: Option<String>,
}

pub async fn enrich_event_images<A: ImageCacheAdapter>(adapter: &A, event: &mut MessageEvent) {
    enrich_message_images(adapter, event.message_id, &mut event.message_list).await;
}

pub async fn enrich_message_images<A: ImageCacheAdapter>(
    adapter: &A,
    message_id: i64,
    messages: &mut [Message],
) {
    let Some(object_storage) = adapter.object_storage().await else {
        debug!("{LOG_PREFIX} Object storage is not configured; skipping image caching");
        return;
    };

    enrich_message_images_with_storage(adapter, &object_storage, message_id, messages).await;
}

#[async_recursion]
async fn enrich_message_images_with_storage<A: ImageCacheAdapter>(
    adapter: &A,
    object_storage: &S3Ref,
    message_id: i64,
    messages: &mut [Message],
) {
    for (segment_index, message) in messages.iter_mut().enumerate() {
        match message {
            Message::Image(image) => {
                if image.rustfs_path().is_some() {
                    continue;
                }

                match cache_one_image(adapter, object_storage, message_id, segment_index, image)
                    .await
                {
                    Ok(Some(rustfs_path)) => {
                        info!(
                            "{LOG_PREFIX} Cached image for message {} segment {} to {}",
                            message_id, segment_index, rustfs_path
                        );
                    }
                    Ok(None) => {
                        debug!(
                            "{LOG_PREFIX} No resolvable source for message {} segment {}",
                            message_id, segment_index
                        );
                    }
                    Err(error) => {
                        warn!(
                            "{LOG_PREFIX} Failed to cache image for message {} segment {}: {}",
                            message_id, segment_index, error
                        );
                        let should_spawn = adapter
                            .enqueue_pending_image(PendingImageUpload {
                                message_id,
                                segment_index,
                                image: image.clone(),
                            })
                            .await;
                        if should_spawn {
                            let adapter_clone = adapter.clone();
                            tokio::spawn(async move {
                                run_retry_loop(adapter_clone).await;
                            });
                        }
                    }
                }
            }
            Message::Forward(forward) => {
                enrich_forward_node_images(
                    adapter,
                    object_storage,
                    message_id,
                    &mut forward.content,
                )
                .await;
            }
            _ => {}
        }
    }
}

#[async_recursion]
async fn enrich_forward_node_images<A: ImageCacheAdapter>(
    adapter: &A,
    object_storage: &S3Ref,
    message_id: i64,
    nodes: &mut [ForwardNodeMessage],
) {
    for node in nodes {
        enrich_message_images_with_storage(adapter, object_storage, message_id, &mut node.content)
            .await;
    }
}

async fn run_retry_loop<A: ImageCacheAdapter>(adapter: A) {
    loop {
        let Some(object_storage) = adapter.object_storage().await else {
            adapter.set_retry_task_running(false).await;
            return;
        };

        let Some(mut pending) = adapter.dequeue_pending_image().await else {
            adapter.set_retry_task_running(false).await;
            return;
        };

        match cache_one_image(
            &adapter,
            &object_storage,
            pending.message_id,
            pending.segment_index,
            &mut pending.image,
        )
        .await
        {
            Ok(Some(_)) => info!(
                "{LOG_PREFIX} Retry succeeded for message {} segment {}",
                pending.message_id, pending.segment_index
            ),
            Ok(None) => warn!(
                "{LOG_PREFIX} Retry still has no source for message {} segment {}",
                pending.message_id, pending.segment_index
            ),
            Err(error) => {
                warn!(
                    "{LOG_PREFIX} Retry failed for message {} segment {}: {}",
                    pending.message_id, pending.segment_index, error
                );
                adapter.enqueue_pending_image(pending).await;
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        }
    }
}

async fn cache_one_image<A: ImageCacheAdapter>(
    adapter: &A,
    object_storage: &S3Ref,
    message_id: i64,
    segment_index: usize,
    image: &mut ImageMessage,
) -> Result<Option<String>> {
    let Some(resolved) = resolve_image_payload(adapter, image).await? else {
        return Ok(None);
    };

    let saved = save_image_to_object_storage(
        object_storage,
        &ImageObjectStorageInput {
            message_id,
            segment_index,
            file_name: resolved.file_name.clone(),
            content_type: resolved.content_type.clone(),
            bytes: resolved.bytes,
        },
    )
    .await?;

    image.media = PersistedMedia::new(
        image.media.source.clone(),
        image.media.original_source.clone(),
        saved.object_key.clone(),
        image
            .media
            .name
            .clone()
            .or_else(|| Some(resolved.file_name.clone())),
        image.media.description.clone(),
        Some(resolved.content_type.clone()),
    );

    Ok(Some(saved.object_key))
}

struct ResolvedImagePayload {
    bytes: Vec<u8>,
    file_name: String,
    content_type: String,
}

async fn resolve_image_payload<A: ImageCacheAdapter>(
    adapter: &A,
    image: &ImageMessage,
) -> Result<Option<ResolvedImagePayload>> {
    if let Some(path) = local_file_source(image) {
        if let Some(payload) = read_local_file(path, image.name()).await? {
            return Ok(Some(payload));
        }
    }

    if let Some(url) = remote_image_source(image) {
        if let Some(payload) = download_remote_file(url, image.name()).await? {
            return Ok(Some(payload));
        }
    }

    if let Some(detail) = fetch_napcat_image_detail(adapter, image).await? {
        if let Some(ref base64_payload) = detail.base64 {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(base64_payload.as_bytes())
                .map_err(|e| Error::ValidationError(format!("invalid image base64: {e}")))?;
            let file_name = detail
                .file_name
                .clone()
                .or_else(|| image.media.name.clone())
                .unwrap_or_else(|| "image.bin".to_string());
            return Ok(Some(ResolvedImagePayload {
                content_type: infer_content_type(&file_name, None),
                bytes,
                file_name,
            }));
        }

        if let Some(ref path) = detail.file {
            if let Some(payload) = read_local_file(path, detail.file_name.as_deref()).await? {
                return Ok(Some(payload));
            }
        }

        if let Some(ref url) = detail.url {
            if let Some(payload) = download_remote_file(url, detail.file_name.as_deref()).await? {
                return Ok(Some(payload));
            }
        }
    }

    Ok(None)
}

async fn read_local_file(
    path: &str,
    preferred_name: Option<&str>,
) -> Result<Option<ResolvedImagePayload>> {
    let file_path = Path::new(path);
    if !file_path.exists() {
        return Ok(None);
    }

    let bytes = tokio::fs::read(file_path).await?;
    let file_name = preferred_name
        .map(ToOwned::to_owned)
        .or_else(|| {
            file_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "image.bin".to_string());

    Ok(Some(ResolvedImagePayload {
        content_type: infer_content_type(&file_name, None),
        bytes,
        file_name,
    }))
}

async fn download_remote_file(
    url: &str,
    preferred_name: Option<&str>,
) -> Result<Option<ResolvedImagePayload>> {
    let response = Client::new().get(url).send().await?;
    if !response.status().is_success() {
        return Ok(None);
    }

    let content_type_header = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = response.bytes().await?.to_vec();
    let file_name = preferred_name
        .map(ToOwned::to_owned)
        .or_else(|| {
            reqwest::Url::parse(url).ok().and_then(|parsed| {
                parsed
                    .path_segments()
                    .and_then(|segments| segments.last().map(ToOwned::to_owned))
            })
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "image.bin".to_string());

    Ok(Some(ResolvedImagePayload {
        content_type: infer_content_type(&file_name, content_type_header.as_deref()),
        bytes,
        file_name,
    }))
}

async fn fetch_napcat_image_detail<A: ImageCacheAdapter>(
    adapter: &A,
    image: &ImageMessage,
) -> Result<Option<NapCatImageDetailData>> {
    let (base_url, token) = adapter.image_detail_request_context().await;

    let file = image
        .original_source()
        .filter(|value| !value.starts_with("http://") && !value.starts_with("https://"))
        .map(ToOwned::to_owned);
    if file.is_none() && remote_image_source(image).is_none() {
        return Ok(None);
    }

    let response = Client::new()
        .post(format!("{}/get_image", base_url.trim_end_matches('/')))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "file_id": file,
            "file": image.original_source(),
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let payload: NapCatImageDetailResponse = response.json().await?;
    Ok(payload.data)
}

fn infer_content_type(file_name: &str, header_content_type: Option<&str>) -> String {
    if let Some(header_content_type) = header_content_type {
        return header_content_type.to_string();
    }

    match Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg".to_string(),
        Some("gif") => "image/gif".to_string(),
        Some("webp") => "image/webp".to_string(),
        Some("bmp") => "image/bmp".to_string(),
        Some("svg") => "image/svg+xml".to_string(),
        _ => "image/png".to_string(),
    }
}

fn local_file_source(image: &ImageMessage) -> Option<&str> {
    image
        .original_source()
        .and_then(|value| value.strip_prefix("file://").or(Some(value)))
        .filter(|value| Path::new(value).exists())
}

fn remote_image_source(image: &ImageMessage) -> Option<&str> {
    image
        .original_source()
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zihuan_core::ims_bot_adapter::models::message::PersistedMediaSource;

    #[test]
    fn infer_persisted_media_from_qq_image_input() {
        let image = ImageMessage::new(PersistedMedia::new(
            PersistedMediaSource::QqChat,
            "https://multimedia.nt.qq.com.cn/download?appid=1407&fileid=test",
            "qq-images/2026/05/16/test.jpg",
            Some("download".to_string()),
            None,
            Some("image/jpeg".to_string()),
        ));

        assert_eq!(image.media.source.to_string(), "qq_chat");
        assert_eq!(
            image.media.original_source,
            "https://multimedia.nt.qq.com.cn/download?appid=1407&fileid=test"
        );
        assert_eq!(image.media.rustfs_path, "qq-images/2026/05/16/test.jpg");
        assert_eq!(image.media.mime_type.as_deref(), Some("image/jpeg"));
    }
}
