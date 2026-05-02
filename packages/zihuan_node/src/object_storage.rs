use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use reqwest::{Client, Method, Response, Url};
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
        let base = if let Some(ref public_base_url) = self.public_base_url {
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
        let request_url = self.request_url_for_key(key)?;
        let response = self
            .signed_request(Method::PUT, request_url, Some(content_type), body)
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

    pub async fn ensure_bucket_exists(&self) -> Result<()> {
        let bucket_url = self.bucket_request_url()?;
        let head = self
            .signed_request(Method::HEAD, bucket_url.clone(), None, &[])
            .await?;

        match head.status() {
            status if status.is_success() => Ok(()),
            reqwest::StatusCode::NOT_FOUND => self.create_bucket(bucket_url).await,
            status => {
                let text = head.text().await.unwrap_or_default();
                Err(Error::ValidationError(format!(
                    "object storage bucket check failed with status {status}: {text}"
                )))
            }
        }
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

    fn bucket_request_url(&self) -> Result<Url> {
        let endpoint = Url::parse(&self.endpoint)
            .map_err(|e| Error::ValidationError(format!("invalid object storage endpoint: {e}")))?;
        let mut url = endpoint;
        if self.path_style {
            url.set_path(self.bucket.trim_matches('/'));
        } else {
            let host = url.host_str().ok_or_else(|| {
                Error::ValidationError("object storage endpoint host is missing".to_string())
            })?;
            url.set_host(Some(&format!("{}.{}", self.bucket, host)))
                .map_err(|e| Error::ValidationError(format!("invalid object storage host: {e}")))?;
            url.set_path("");
        }
        Ok(url)
    }

    async fn create_bucket(&self, bucket_url: Url) -> Result<()> {
        let response = self
            .signed_request(Method::PUT, bucket_url, None, &[])
            .await?;

        match response.status() {
            status if status.is_success() || status == reqwest::StatusCode::CONFLICT => Ok(()),
            status => {
                let text = response.text().await.unwrap_or_default();
                Err(Error::ValidationError(format!(
                    "object storage bucket create failed with status {status}: {text}"
                )))
            }
        }
    }

    async fn signed_request(
        &self,
        method: Method,
        request_url: Url,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Result<Response> {
        let client = Client::new();
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
            "{}\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
            method.as_str()
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

        let mut request = client
            .request(method, request_url)
            .header(HOST, host)
            .header("x-amz-date", amz_date)
            .header("x-amz-content-sha256", payload_hash)
            .header(AUTHORIZATION, authorization);
        if let Some(content_type) = content_type {
            request = request.header(CONTENT_TYPE, content_type);
        }
        if !body.is_empty() {
            request = request.body(body.to_vec());
        }

        request.send().await.map_err(Into::into)
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
