use base64::Engine;
use log::{info, warn};
use reqwest::header::CONTENT_TYPE;
use std::path::Path;
use std::time::Duration;
use tokio::task::block_in_place;
use zihuan_core::llm::ContentPart;
use zihuan_graph_engine::object_storage::S3Ref;

use crate::models::message::ImageMessage;

const IMAGE_DOWNLOAD_TIMEOUT_SECS: u64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePartSource {
    LocalFile,
    ObjectStorage,
    DownloadedRemote,
    UploadedToS3,
    DataUrl,
}

#[derive(Debug, Clone)]
pub struct ResolvedImagePart {
    pub part: ContentPart,
    pub source: ImagePartSource,
}

#[derive(Debug, Clone)]
pub enum ResolvedTextSegment {
    Text(String),
    Image(ResolvedImagePart),
}

#[derive(Debug)]
struct DownloadedRemoteImage {
    bytes: Vec<u8>,
    content_type: Option<String>,
}

fn infer_content_type(file_name: &str) -> &'static str {
    match Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        Some("avif") => "image/avif",
        _ => "image/png",
    }
}

fn image_name(image: &ImageMessage) -> &str {
    image
        .name()
        .or_else(|| {
            image
                .original_source()
                .and_then(|path| path.strip_prefix("file://").or(Some(path)))
                .and_then(|path| Path::new(path).file_name())
                .and_then(|name| name.to_str())
        })
        .unwrap_or("image.png")
}

fn image_part_from_bytes_with_mime(mime_type: &str, bytes: Vec<u8>) -> ContentPart {
    let base64_payload = base64::engine::general_purpose::STANDARD.encode(bytes);
    ContentPart::image_data_url(mime_type, base64_payload)
}

fn image_part_from_bytes(image: &ImageMessage, bytes: Vec<u8>) -> ContentPart {
    image_part_from_bytes_with_mime(
        image
            .mime_type()
            .unwrap_or_else(|| infer_content_type(image_name(image))),
        bytes,
    )
}

fn sanitize_object_storage_key_fragment(value: &str, max_len: usize) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('/');
    if trimmed.is_empty() {
        return "image".to_string();
    }

    trimmed.chars().take(max_len).collect()
}

fn derive_multimodal_cache_key(url: &str, file_name_hint: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let source_fragment = sanitize_object_storage_key_fragment(without_scheme, 160);
    let file_name_fragment = sanitize_object_storage_key_fragment(file_name_hint, 80);
    format!(
        "qq-images/multimodal-cache/{}_{}",
        source_fragment, file_name_fragment
    )
}

fn lower_content_type_is_image(content_type: &str) -> bool {
    content_type
        .split(';')
        .next()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| value.starts_with("image/"))
}

fn content_type_from_url_hint(url: &str) -> Option<&'static str> {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let path = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let ext = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match ext.as_deref() {
        Some("jpg") | Some("jpeg") => return Some("image/jpeg"),
        Some("png") => return Some("image/png"),
        Some("gif") => return Some("image/gif"),
        Some("webp") => return Some("image/webp"),
        Some("bmp") => return Some("image/bmp"),
        Some("svg") => return Some("image/svg+xml"),
        Some("avif") => return Some("image/avif"),
        _ => {}
    }

    let query = without_fragment.split_once('?')?.1;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key.eq_ignore_ascii_case("format") || key.eq_ignore_ascii_case("ext") {
            match value.to_ascii_lowercase().as_str() {
                "jpg" | "jpeg" => return Some("image/jpeg"),
                "png" => return Some("image/png"),
                "gif" => return Some("image/gif"),
                "webp" => return Some("image/webp"),
                "bmp" => return Some("image/bmp"),
                "svg" => return Some("image/svg+xml"),
                "avif" => return Some("image/avif"),
                _ => {}
            }
        }
    }

    None
}

fn infer_file_name_from_url(url: &str, mime_type: Option<&str>) -> String {
    let path = url.split('#').next().unwrap_or(url);
    let path = path.split('?').next().unwrap_or(path);
    if let Some(name) = Path::new(path).file_name().and_then(|value| value.to_str()) {
        if !name.trim().is_empty() {
            return name.to_string();
        }
    }

    let extension = mime_type.and_then(|value| {
        match value
            .split(';')
            .next()
            .map(|item| item.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("image/jpeg") => Some("jpg"),
            Some("image/png") => Some("png"),
            Some("image/gif") => Some("gif"),
            Some("image/webp") => Some("webp"),
            Some("image/bmp") => Some("bmp"),
            Some("image/svg+xml") => Some("svg"),
            Some("image/avif") => Some("avif"),
            _ => None,
        }
    });

    match extension {
        Some(ext) => format!("remote_image.{ext}"),
        None => "remote_image".to_string(),
    }
}

fn http_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(IMAGE_DOWNLOAD_TIMEOUT_SECS))
        .build()
        .ok()
}

async fn probe_remote_image_content_type(url: &str, log_prefix: &str) -> Option<String> {
    let Some(client) = http_client() else {
        warn!("{log_prefix} failed to build HTTP client while probing remote image url={url}");
        return None;
    };

    let response = match client.head(url).send().await {
        Ok(response) => response,
        Err(error) => {
            warn!(
                "{log_prefix} failed to probe remote image HEAD url={}: {}",
                url, error
            );
            return None;
        }
    };

    if !response.status().is_success() {
        return None;
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())?;
    lower_content_type_is_image(&content_type).then_some(content_type)
}

async fn download_remote_image(url: &str, log_prefix: &str) -> Option<DownloadedRemoteImage> {
    let Some(client) = http_client() else {
        warn!("{log_prefix} failed to build HTTP client while downloading remote image url={url}");
        return None;
    };

    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(error) => {
            warn!(
                "{log_prefix} failed to download remote image for multimodal input url={}: {}",
                url, error
            );
            return None;
        }
    };

    if !response.status().is_success() {
        warn!(
            "{log_prefix} remote image returned non-success status for multimodal input url={} status={}",
            url,
            response.status()
        );
        return None;
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string());

    match response.bytes().await {
        Ok(bytes) => Some(DownloadedRemoteImage {
            bytes: bytes.to_vec(),
            content_type,
        }),
        Err(error) => {
            warn!(
                "{log_prefix} failed to read remote image body for multimodal input url={}: {}",
                url, error
            );
            None
        }
    }
}

fn run_async<T>(future: impl std::future::Future<Output = T>) -> Option<T> {
    if tokio::runtime::Handle::try_current().is_ok() {
        Some(block_in_place(|| {
            tokio::runtime::Handle::current().block_on(future)
        }))
    } else {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()
            .map(|runtime| runtime.block_on(future))
    }
}

fn trim_trailing_url_punctuation(candidate: &str) -> (&str, &str) {
    let trimmed = candidate.trim_end_matches([
        '.', ',', '!', '?', ';', ':', ')', ']', '}', '>', '"', '\'', '，', '。', '！', '？', '；',
        '：', '）', '】', '》', '、',
    ]);
    let suffix = &candidate[trimmed.len()..];
    (trimmed, suffix)
}

fn next_url_start(text: &str, offset: usize) -> Option<usize> {
    let remaining = &text[offset..];
    let http = remaining.find("http://").map(|index| offset + index);
    let https = remaining.find("https://").map(|index| offset + index);
    match (http, https) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(index), None) | (None, Some(index)) => Some(index),
        (None, None) => None,
    }
}

fn resolve_remote_url_as_image_part(
    url: &str,
    file_name_hint: &str,
    mime_type_hint: Option<&str>,
    s3_ref: Option<&S3Ref>,
    cache_to_s3: bool,
    log_prefix: &str,
) -> Option<ResolvedImagePart> {
    if url.starts_with("data:") {
        return Some(ResolvedImagePart {
            part: ContentPart::image_url_string(url.to_string()),
            source: ImagePartSource::DataUrl,
        });
    }

    let verified_image_content_type = mime_type_hint
        .map(ToOwned::to_owned)
        .or_else(|| content_type_from_url_hint(url).map(ToOwned::to_owned))
        .or_else(|| run_async(probe_remote_image_content_type(url, log_prefix)).flatten())?;

    let downloaded = run_async(download_remote_image(url, log_prefix)).flatten()?;
    let final_content_type = match downloaded.content_type.as_deref() {
        Some(value) if lower_content_type_is_image(value) => value.to_string(),
        Some(value) => {
            warn!(
                "{log_prefix} remote URL looked like an image but GET returned non-image content-type url={} content_type={}",
                url, value
            );
            return None;
        }
        None => verified_image_content_type,
    };

    let part = image_part_from_bytes_with_mime(&final_content_type, downloaded.bytes.clone());

    if cache_to_s3 {
        if let Some(s3_ref) = s3_ref {
            let object_key = derive_multimodal_cache_key(url, file_name_hint);
            let s3_ref = s3_ref.clone();
            let bytes = downloaded.bytes.clone();
            let content_type = final_content_type.clone();
            match run_async(
                async move { s3_ref.put_object(&object_key, &content_type, &bytes).await },
            ) {
                Some(Ok(object_url)) => {
                    info!(
                        "{log_prefix} cached remote image to object storage for multimodal input url={} object_url={}",
                        url, object_url
                    );
                    return Some(ResolvedImagePart {
                        part,
                        source: ImagePartSource::UploadedToS3,
                    });
                }
                Some(Err(error)) => {
                    warn!(
                        "{log_prefix} failed to cache remote image to object storage for multimodal input url={}: {}",
                        url, error
                    );
                }
                None => {
                    warn!(
                        "{log_prefix} failed to create runtime while caching remote image to object storage url={}",
                        url
                    );
                }
            }
        }
    }

    Some(ResolvedImagePart {
        part,
        source: ImagePartSource::DownloadedRemote,
    })
}

pub fn resolve_image_message_part(
    image: &ImageMessage,
    s3_ref: Option<&S3Ref>,
    cache_remote_to_s3: bool,
    log_prefix: &str,
) -> Option<ResolvedImagePart> {
    if let Some(local_path) = image
        .original_source()
        .and_then(|value| value.strip_prefix("file://").or(Some(value)))
    {
        let file_path = Path::new(local_path);
        if file_path.exists() {
            match std::fs::read(file_path) {
                Ok(bytes) => {
                    return Some(ResolvedImagePart {
                        part: image_part_from_bytes(image, bytes),
                        source: ImagePartSource::LocalFile,
                    });
                }
                Err(error) => {
                    warn!(
                        "{log_prefix} failed to read image file for multimodal input path={}: {}",
                        local_path, error
                    );
                }
            }
        }
    }

    if let (Some(s3_ref), Some(object_key)) = (s3_ref, image.rustfs_path()) {
        let s3_ref = s3_ref.clone();
        let key = object_key.to_string();
        match run_async(async move { s3_ref.get_object_bytes(&key).await }) {
            Some(Ok(bytes)) => {
                return Some(ResolvedImagePart {
                    part: image_part_from_bytes(image, bytes),
                    source: ImagePartSource::ObjectStorage,
                });
            }
            Some(Err(error)) => {
                warn!(
                    "{log_prefix} failed to read object storage image for multimodal input object_key={}: {}",
                    object_key, error
                );
            }
            None => {
                warn!(
                    "{log_prefix} failed to create runtime for object storage image read object_key={}",
                    object_key
                );
            }
        }
    }

    if let Some(direct_url) = image.original_source() {
        if let Some(part) = resolve_remote_url_as_image_part(
            direct_url,
            image_name(image),
            image.mime_type(),
            s3_ref,
            cache_remote_to_s3 && image.rustfs_path().is_none(),
            log_prefix,
        ) {
            return Some(part);
        }
    }

    warn!(
        "{log_prefix} skipping multimodal image because no safe source could be resolved: {}",
        image
    );
    None
}

pub fn resolve_plain_text_segments(
    text: &str,
    s3_ref: Option<&S3Ref>,
    cache_to_s3: bool,
    log_prefix: &str,
) -> Vec<ResolvedTextSegment> {
    let mut segments = Vec::new();
    let mut offset = 0usize;

    while let Some(start) = next_url_start(text, offset) {
        if start > offset {
            segments.push(ResolvedTextSegment::Text(text[offset..start].to_string()));
        }

        let remaining = &text[start..];
        let end = remaining
            .find(char::is_whitespace)
            .map(|index| start + index)
            .unwrap_or(text.len());
        let candidate = &text[start..end];
        let (trimmed_url, trailing) = trim_trailing_url_punctuation(candidate);

        if !trimmed_url.is_empty() {
            let file_name_hint =
                infer_file_name_from_url(trimmed_url, content_type_from_url_hint(trimmed_url));
            if let Some(resolved) = resolve_remote_url_as_image_part(
                trimmed_url,
                &file_name_hint,
                None,
                s3_ref,
                cache_to_s3,
                log_prefix,
            ) {
                segments.push(ResolvedTextSegment::Image(resolved));
            } else {
                segments.push(ResolvedTextSegment::Text(trimmed_url.to_string()));
            }
        }

        if !trailing.is_empty() {
            segments.push(ResolvedTextSegment::Text(trailing.to_string()));
        }

        offset = end;
    }

    if offset < text.len() {
        segments.push(ResolvedTextSegment::Text(text[offset..].to_string()));
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::{content_type_from_url_hint, trim_trailing_url_punctuation};

    #[test]
    fn content_type_hint_supports_query_format() {
        assert_eq!(
            content_type_from_url_hint("https://pbs.twimg.com/media/demo?format=png&name=large"),
            Some("image/png")
        );
    }

    #[test]
    fn trim_trailing_url_punctuation_preserves_suffix() {
        let (url, suffix) = trim_trailing_url_punctuation("https://example.com/a.png).");

        assert_eq!(url, "https://example.com/a.png");
        assert_eq!(suffix, ").");
    }
}
