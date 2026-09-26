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
    validate_connection_authentication(
        auth_method,
        username.as_deref(),
        password.as_deref(),
        api_key.as_deref(),
        "weaviate",
    )?;
    let weaviate_ref = Arc::new(WeaviateRef::new(
        base_url,
        schema.weaviate_class_name(),
        username,
        password,
        api_key,
        std::time::Duration::from_secs(30),
    )?);
    if !weaviate_ref.ready()? {
        return Err(crate::string_error!("Weaviate is reachable but not ready yet"));
    }
    ensure_collection_schema(&weaviate_ref, schema, true)?;
    Ok(weaviate_ref)
}
