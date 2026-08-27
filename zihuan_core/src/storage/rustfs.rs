use std::sync::Arc;

use crate::error::Result;
use crate::graph::object_storage::S3Ref;

pub async fn build_s3_ref(
    endpoint: &str,
    bucket: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
    public_base_url: Option<String>,
    path_style: bool,
) -> Result<Arc<S3Ref>> {
    let s3_ref = Arc::new(S3Ref {
        endpoint: endpoint.to_string(),
        bucket: bucket.to_string(),
        access_key: access_key.to_string(),
        secret_key: secret_key.to_string(),
        region: region.to_string(),
        public_base_url,
        path_style,
    });

    ensure_endpoint_bypasses_proxy(endpoint);
    s3_ref.ensure_bucket_exists().await?;
    Ok(s3_ref)
}

fn ensure_endpoint_bypasses_proxy(endpoint: &str) {
    let Some(host) = extract_host(endpoint) else {
        return;
    };

    append_no_proxy_var("NO_PROXY", &host);
    append_no_proxy_var("no_proxy", &host);
}

fn append_no_proxy_var(var_name: &str, host: &str) -> bool {
    let current = std::env::var(var_name).unwrap_or_default();
    let already_present = current.split(',').map(str::trim).any(|entry| entry.eq_ignore_ascii_case(host));
    if already_present {
        return false;
    }

    let updated = if current.trim().is_empty() {
        host.to_string()
    } else {
        format!("{current},{host}")
    };
    std::env::set_var(var_name, updated);
    true
}

fn extract_host(endpoint: &str) -> Option<String> {
    let endpoint = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);
    let authority = endpoint.split('/').next()?.trim();
    if authority.is_empty() {
        return None;
    }
    let host = authority
        .rsplit_once('@')
        .map(|(_, suffix)| suffix)
        .unwrap_or(authority)
        .split(':')
        .next()
        .unwrap_or("")
        .trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}
