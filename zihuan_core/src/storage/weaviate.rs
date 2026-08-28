use std::sync::Arc;

use crate::error::Result;
use crate::weaviate::{WeaviateCollectionSchema, WeaviateRef};

use crate::storage::weaviate_schema::ensure_collection_schema;
use crate::storage::{validate_connection_authentication, ConnectionAuthMethod};

pub fn build_weaviate_ref(
    base_url: &str,
    class_name: &str,
    username: Option<String>,
    password: Option<String>,
    api_key: Option<String>,
    auth_method: ConnectionAuthMethod,
    collection_schema: WeaviateCollectionSchema,
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
        class_name,
        username,
        password,
        api_key,
        std::time::Duration::from_secs(30),
    )?);
    if !weaviate_ref.ready()? {
        return Err(crate::string_error!("Weaviate is reachable but not ready yet"));
    }
    ensure_collection_schema(&weaviate_ref, collection_schema, true)?;
    Ok(weaviate_ref)
}
