use crate::connection::Connection;
use std::sync::Arc;

use crate::error::Result;
use crate::retrieval::RetrievalSchema;
use crate::weaviate::WeaviateRef;

use crate::storage::weaviate_schema::ensure_collection_schema;
use crate::storage::{validate_connection_authentication, ConnectionAuthMethod};

/// Builds a live Weaviate reference bound to one retrieval schema's class.
pub fn build_weaviate_ref(
    base_url: &str,
    schema: RetrievalSchema,
    username: Option<String>,
    password: Option<String>,
    api_key: Option<String>,
    auth_method: ConnectionAuthMethod,
) -> Result<Arc<WeaviateRef>> {
    crate::runtime::block_async(WeaviateRef::from_config(
        &crate::storage::WeaviateConnection {
            base_url: base_url.to_string(),
            username,
            password,
            api_key,
            auth_method,
        },
        schema,
    ))
    .map(Arc::new)
}

impl WeaviateRef {
    pub async fn from_config(
        config: &crate::storage::WeaviateConnection,
        schema: RetrievalSchema,
    ) -> Result<Self> {
        validate_connection_authentication(
            config.auth_method,
            config.username.as_deref(),
            config.password.as_deref(),
            config.api_key.as_deref(),
            "weaviate",
        )?;
        let mut connection = Self::new(
            &config.base_url,
            schema.weaviate_class_name(),
            config.username.clone(),
            config.password.clone(),
            config.api_key.clone(),
            std::time::Duration::from_secs(30),
        )?;
        connection.connect().await?;
        ensure_collection_schema(&connection, schema, true)?;
        Ok(connection)
    }
}
