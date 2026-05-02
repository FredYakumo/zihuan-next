use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use reqwest::{Client, Url};
use sha2::{Digest, Sha256};
use std::fmt;
use zihuan_core::error::{Error, Result};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct S3Ref {
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub public_base_url: Option<String>,
    pub path_style: bool,
}

impl S3Ref {
    pub fn object_url_for_key(&self, key: &str) -> Result<String> {
        let base = if let Some(public_base_url) = self.public_base_url.as_deref() {
            public_base_url.trim_end_matches('/').to_string()
        } else if self.path_style {
            format!(
                "{}/{}",
                self.endpoint.trim_end_matches('/'),
                self.bucket.trim_matches('/')
            )
        } else {
            let endpoint = Url::parse(&self.endpoint).map_err(|e| {
                Error::ValidationError(format!("invalid object storage endpoint: {e}"))
            })?;
            let host = endpoint.host_str().ok_or_else(|| {
                Error::ValidationError("object storage endpoint host is missing".to_string())
            })?;
            let scheme = endpoint.scheme();
            format!("{scheme}://{}.{}", self.bucket, host)
        };

        Ok(format!("{base}/{}", key.trim_start_matches('/')))
    }

    pub async fn put_object(&self, key: &str, content_type: &str, body: &[u8]) -> Result<String> {
        let client = Client::new();
        let request_url = self.request_url_for_key(key)?;
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let payload_hash = hex_sha256(body);
        let host = request_url
            .host_str()
            .ok_or_else(|| {
                Error::ValidationError("object storage request host is missing".to_string())
            })?
            .to_string();
        let canonical_uri = canonical_uri(&request_url);
        let canonical_headers =
            format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "PUT\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );
        let credential_scope = format!("{date_stamp}/{}/s3/aws4_request", self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
            hex_sha256(canonical_request.as_bytes())
        );
        let signing_key = signing_key(&self.secret_key, &date_stamp, &self.region, "s3")?;
        let signature = hex::encode(hmac_sign(&signing_key, string_to_sign.as_bytes())?);
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.access_key, credential_scope, signed_headers, signature
        );

        let response = client
            .put(request_url.clone())
            .header(HOST, host)
            .header("x-amz-date", amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header(AUTHORIZATION, authorization)
            .header(CONTENT_TYPE, content_type)
            .body(body.to_vec())
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(Error::ValidationError(format!(
                "object storage PUT failed with status {status}: {text}"
            )));
        }

        self.object_url_for_key(key)
    }

    fn request_url_for_key(&self, key: &str) -> Result<Url> {
        let endpoint = Url::parse(&self.endpoint)
            .map_err(|e| Error::ValidationError(format!("invalid object storage endpoint: {e}")))?;
        let mut url = endpoint;
        if self.path_style {
            let path = format!(
                "{}/{}",
                self.bucket.trim_matches('/'),
                key.trim_start_matches('/')
            );
            url.set_path(&path);
        } else {
            let host = url.host_str().ok_or_else(|| {
                Error::ValidationError("object storage endpoint host is missing".to_string())
            })?;
            url.set_host(Some(&format!("{}.{}", self.bucket, host)))
                .map_err(|e| Error::ValidationError(format!("invalid object storage host: {e}")))?;
            url.set_path(key.trim_start_matches('/'));
        }
        Ok(url)
    }
}

impl fmt::Debug for S3Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Ref")
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("access_key", &"<redacted>")
            .field("secret_key", &"<redacted>")
            .field("region", &self.region)
            .field("public_base_url", &self.public_base_url)
            .field("path_style", &self.path_style)
            .finish()
    }
}

fn hmac_sign(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| Error::ValidationError(format!("invalid hmac key: {e}")))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Result<Vec<u8>> {
    let k_date = hmac_sign(format!("AWS4{secret}").as_bytes(), date.as_bytes())?;
    let k_region = hmac_sign(&k_date, region.as_bytes())?;
    let k_service = hmac_sign(&k_region, service.as_bytes())?;
    hmac_sign(&k_service, b"aws4_request")
}

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn canonical_uri(url: &Url) -> String {
    let path = url.path();
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}
