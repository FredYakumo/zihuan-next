use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::{DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

use crate::RuntimeStorageConnectionManager;

const CONFIG_ID_FIELD: &str = "config_id";
const LEGACY_CONNECTION_ID_FIELD: &str = "connection_id";

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
    let already_present = current
        .split(',')
        .map(str::trim)
        .any(|entry| entry.eq_ignore_ascii_case(host));
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

pub struct RustfsNode {
    id: String,
    name: String,
    config_id: Option<String>,
}

impl RustfsNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            config_id: None,
        }
    }

    fn connection_select_field() -> NodeConfigField {
        NodeConfigField::new(
            CONFIG_ID_FIELD,
            DataType::String,
            NodeConfigWidget::ConnectionSelect,
        )
        .with_connection_kind("rustfs")
        .with_description("选择系统中的 RustFS 对象存储连接配置")
    }

    fn selected_config_id(&self) -> Result<&str> {
        self
            .config_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ValidationError("config_id is required".to_string()))
    }
}

impl Node for RustfsNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("RustFS 对象存储配置 - 从系统连接中选择并输出 S3Ref")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![Port::new("s3_ref", DataType::S3Ref).with_description("对象存储引用")]
    }

    fn config_fields(&self) -> Vec<NodeConfigField> {
        vec![Self::connection_select_field()]
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        self.config_id = inline_values
            .get(CONFIG_ID_FIELD)
            .or_else(|| inline_values.get(LEGACY_CONNECTION_ID_FIELD))
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.clone()),
                _ => None,
            });
        Ok(())
    }

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let config_id = self.selected_config_id()?;
        let s3_ref = zihuan_core::runtime::block_async(
            RuntimeStorageConnectionManager::shared().get_or_create_s3_ref(config_id),
        )?;

        Ok(HashMap::from([(
            "s3_ref".to_string(),
            DataValue::S3Ref(s3_ref),
        )]))
    }
}
