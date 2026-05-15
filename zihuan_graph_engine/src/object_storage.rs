use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::Builder as S3ConfigBuilder;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{BucketLocationConstraint, CreateBucketConfiguration};
use aws_sdk_s3::Client as S3Client;
use aws_types::region::Region;
use reqwest::Url;
use std::fmt;
use zihuan_core::error::{Error, Result};
use zihuan_core::url_utils::pct_encode;

#[derive(Clone)]
pub struct S3Ref {
    pub endpoint: String,
    pub endpoint_username: Option<String>,
    pub endpoint_password: Option<String>,
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
        let client = self.s3_client().await?;
        client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(body.to_vec()))
            .send()
            .await
            .map_err(|e| Error::ValidationError(format!("object storage PUT failed: {}", e)))?;

        self.object_url_for_key(key)
    }

    pub async fn get_object_bytes(&self, key: &str) -> Result<Vec<u8>> {
        let client = self.s3_client().await?;
        let response = client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Error::ValidationError(format!("object storage GET failed: {}", e)))?;

        let body = response.body.collect().await.map_err(|e| {
            Error::ValidationError(format!("object storage body read failed: {}", e))
        })?;

        Ok(body.into_bytes().to_vec())
    }

    pub async fn list_objects(
        &self,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        max_keys: Option<i32>,
    ) -> Result<aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Output> {
        let client = self.s3_client().await?;
        let mut request = client.list_objects_v2().bucket(&self.bucket);
        if let Some(p) = prefix {
            request = request.prefix(p);
        }
        if let Some(d) = delimiter {
            request = request.delimiter(d);
        }
        if let Some(mk) = max_keys {
            request = request.max_keys(mk);
        }
        request
            .send()
            .await
            .map_err(|e| Error::ValidationError(format!("S3 list_objects failed: {}", e)))
    }

    pub async fn ensure_bucket_exists(&self) -> Result<()> {
        let client = self.s3_client().await?;
        let region = self.normalized_region();
        let mut request = client.create_bucket().bucket(&self.bucket);
        if region != "us-east-1" {
            request = request.create_bucket_configuration(
                CreateBucketConfiguration::builder()
                    .location_constraint(BucketLocationConstraint::from(region))
                    .build(),
            );
        }
        let result = request.send().await;

        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                let message = error.to_string();
                if message.contains("BucketAlreadyOwnedByYou")
                    || message.contains("BucketAlreadyExists")
                    || message.contains("OperationAborted")
                {
                    return Ok(());
                }
                if client
                    .head_bucket()
                    .bucket(&self.bucket)
                    .send()
                    .await
                    .is_ok()
                {
                    return Ok(());
                }
                Err(Error::ValidationError(format!(
                    "object storage bucket create failed: {}",
                    message
                )))
            }
        }
    }

    async fn s3_client(&self) -> Result<S3Client> {
        let credentials = Credentials::new(
            self.access_key.clone(),
            self.secret_key.clone(),
            None,
            None,
            "zihuan-next",
        );
        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(self.region.clone()))
            .credentials_provider(credentials)
            .endpoint_url(self.endpoint_url_with_auth()?)
            .load()
            .await;
        let config = S3ConfigBuilder::from(&shared_config)
            .force_path_style(self.path_style)
            .build();
        Ok(S3Client::from_conf(config))
    }

    fn normalized_region(&self) -> &str {
        let region = self.region.trim();
        if region.is_empty() {
            "us-east-1"
        } else {
            region
        }
    }

    fn endpoint_url_with_auth(&self) -> Result<String> {
        let username = self
            .endpoint_username
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let password = self
            .endpoint_password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if username.is_none() && password.is_none() {
            return Ok(self.endpoint.clone());
        }

        let mut url = Url::parse(&self.endpoint).map_err(|err| {
            Error::ValidationError(format!(
                "invalid object storage endpoint '{}': {}",
                self.endpoint, err
            ))
        })?;
        let encoded_username = username.map(pct_encode).unwrap_or_default();
        url.set_username(&encoded_username).map_err(|_| {
            Error::ValidationError(format!(
                "failed to apply username to object storage endpoint '{}'",
                self.endpoint
            ))
        })?;
        url.set_password(password.map(pct_encode).as_deref())
            .map_err(|_| {
                Error::ValidationError(format!(
                    "failed to apply password to object storage endpoint '{}'",
                    self.endpoint
                ))
            })?;
        Ok(url.to_string())
    }
}

impl fmt::Debug for S3Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Ref")
            .field("endpoint", &self.endpoint)
            .field("endpoint_username", &self.endpoint_username)
            .field(
                "endpoint_password",
                &self.endpoint_password.as_ref().map(|_| "<redacted>"),
            )
            .field("bucket", &self.bucket)
            .field("access_key", &"<redacted>")
            .field("secret_key", &"<redacted>")
            .field("region", &self.region)
            .field("public_base_url", &self.public_base_url)
            .field("path_style", &self.path_style)
            .finish()
    }
}
