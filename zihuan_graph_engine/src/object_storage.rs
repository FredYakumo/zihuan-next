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
        request.send().await.map_err(|e| {
            Error::ValidationError(format!("S3 list_objects failed: {}", e))
        })
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
            .endpoint_url(self.endpoint.clone())
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
