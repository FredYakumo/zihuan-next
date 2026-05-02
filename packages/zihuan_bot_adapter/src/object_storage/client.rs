use std::env;
use std::sync::Arc;
use zihuan_node::object_storage::S3Ref;

#[derive(Debug, Clone)]
pub struct ObjectStorageConfig {
    inner: Arc<S3Ref>,
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

    pub fn object_url_for_key(&self, key: &str) -> zihuan_core::error::Result<String> {
        self.inner.object_url_for_key(key)
    }

    pub async fn put_object(
        &self,
        key: &str,
        content_type: &str,
        body: &[u8],
    ) -> zihuan_core::error::Result<String> {
        self.inner.put_object(key, content_type, body).await
    }
}
