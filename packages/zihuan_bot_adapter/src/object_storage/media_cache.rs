use crate::adapter::SharedBotAdapter;
use crate::models::MessageEvent;
use base64::Engine;
use chrono::{Datelike, Utc};
use log::{debug, info, warn};
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::Ordering;
use zihuan_bot_types::message::{ImageMessage, Message};
use zihuan_core::error::Result;
use zihuan_node::object_storage::S3Ref;

const LOG_PREFIX: &str = "[media_cache]";

#[derive(Debug, Clone)]
pub struct PendingImageUpload {
    pub message_id: i64,
    pub segment_index: usize,
    pub image: ImageMessage,
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

pub async fn enrich_event_images(adapter: &SharedBotAdapter, event: &mut MessageEvent) {
    let object_storage = {
        let guard = adapter.lock().await;
        guard.object_storage.clone()
    };

    let Some(object_storage) = object_storage else {
        debug!("{LOG_PREFIX} Object storage is not configured; skipping image caching");
        return;
    };

    for (segment_index, message) in event.message_list.iter_mut().enumerate() {
        let Message::Image(image) = message else {
            continue;
        };

        if image.object_key.is_some() {
            continue;
        }

        match cache_one_image(
            adapter,
            &object_storage,
            event.message_id,
            segment_index,
            image,
        )
        .await
        {
            Ok(Some(object_url)) => {
                info!(
                    "{LOG_PREFIX} Cached image for message {} segment {} to {}",
                    event.message_id, segment_index, object_url
                );
            }
            Ok(None) => {
                debug!(
                    "{LOG_PREFIX} No resolvable source for message {} segment {}",
                    event.message_id, segment_index
                );
            }
            Err(error) => {
                warn!(
                    "{LOG_PREFIX} Failed to cache image for message {} segment {}: {}",
                    event.message_id, segment_index, error
                );
                image.cache_status = Some("pending_retry".to_string());
                enqueue_retry(
                    adapter,
                    PendingImageUpload {
                        message_id: event.message_id,
                        segment_index,
                        image: image.clone(),
                    },
                )
                .await;
            }
        }
    }
}

async fn cache_one_image(
    adapter: &SharedBotAdapter,
    object_storage: &S3Ref,
    message_id: i64,
    segment_index: usize,
    image: &mut ImageMessage,
) -> Result<Option<String>> {
    let Some(resolved) = resolve_image_payload(adapter, image).await? else {
        image.cache_status = Some("source_unavailable".to_string());
        return Ok(None);
    };

    let key = build_object_key(message_id, segment_index, &resolved.file_name);
    let object_url = object_storage
        .put_object(&key, &resolved.content_type, &resolved.bytes)
        .await?;

    image.object_key = Some(key);
    image.object_url = Some(object_url.clone());
    image.local_path = resolved.local_path;
    image.cache_status = Some("uploaded".to_string());
    if image.name.is_none() {
        image.name = Some(resolved.file_name);
    }

    Ok(Some(object_url))
}

async fn enqueue_retry(adapter: &SharedBotAdapter, pending: PendingImageUpload) {
    let should_spawn = {
        let guard = adapter.lock().await;
        let mut queue = guard.pending_image_uploads.lock().await;
        queue.push_back(pending);
        !guard.image_retry_task_running.swap(true, Ordering::SeqCst)
    };

    if should_spawn {
        let adapter_clone = adapter.clone();
        tokio::spawn(async move {
            run_retry_loop(adapter_clone).await;
        });
    }
}

async fn run_retry_loop(adapter: SharedBotAdapter) {
    loop {
        let (object_storage, pending) = {
            let guard = adapter.lock().await;
            let pending = guard.pending_image_uploads.lock().await.pop_front();
            (guard.object_storage.clone(), pending)
        };

        let Some(object_storage) = object_storage else {
            let guard = adapter.lock().await;
            guard
                .image_retry_task_running
                .store(false, Ordering::SeqCst);
            return;
        };

        let Some(mut pending) = pending else {
            let guard = adapter.lock().await;
            guard
                .image_retry_task_running
                .store(false, Ordering::SeqCst);
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
                {
                    let guard = adapter.lock().await;
                    let mut queue = guard.pending_image_uploads.lock().await;
                    queue.push_back(pending);
                }
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        }
    }
}

struct ResolvedImagePayload {
    bytes: Vec<u8>,
    file_name: String,
    content_type: String,
    local_path: Option<String>,
}

async fn resolve_image_payload(
    adapter: &SharedBotAdapter,
    image: &ImageMessage,
) -> Result<Option<ResolvedImagePayload>> {
    if let Some(ref path) = image.path {
        if let Some(payload) = read_local_file(path, image.name.as_deref()).await? {
            return Ok(Some(payload));
        }
    }

    if let Some(ref file) = image.file {
        if let Some(stripped) = file.strip_prefix("file://") {
            if let Some(payload) = read_local_file(stripped, image.name.as_deref()).await? {
                return Ok(Some(payload));
            }
        }
    }

    if let Some(ref url) = image.url {
        if let Some(payload) = download_remote_file(url, image.name.as_deref()).await? {
            return Ok(Some(payload));
        }
    }

    if let Some(detail) = fetch_napcat_image_detail(adapter, image).await? {
        if let Some(ref base64_payload) = detail.base64 {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(base64_payload.as_bytes())
                .map_err(|e| {
                    zihuan_core::error::Error::ValidationError(format!("invalid image base64: {e}"))
                })?;
            let file_name = detail
                .file_name
                .clone()
                .or_else(|| image.name.clone())
                .unwrap_or_else(|| "image.bin".to_string());
            return Ok(Some(ResolvedImagePayload {
                content_type: infer_content_type(&file_name, None),
                bytes,
                file_name,
                local_path: None,
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
        local_path: Some(path.to_string()),
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
        local_path: None,
    }))
}

async fn fetch_napcat_image_detail(
    adapter: &SharedBotAdapter,
    image: &ImageMessage,
) -> Result<Option<NapCatImageDetailData>> {
    let (base_url, token) = {
        let guard = adapter.lock().await;
        (guard.get_http_base_url(), guard.get_token().to_string())
    };

    let file = image.file.clone();
    if file.is_none() && image.url.is_none() {
        return Ok(None);
    }

    let response = Client::new()
        .post(format!("{}/get_image", base_url.trim_end_matches('/')))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "file_id": file,
            "file": image.file,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let payload: NapCatImageDetailResponse = response.json().await?;
    Ok(payload.data)
}

fn build_object_key(message_id: i64, segment_index: usize, file_name: &str) -> String {
    let now = Utc::now();
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
