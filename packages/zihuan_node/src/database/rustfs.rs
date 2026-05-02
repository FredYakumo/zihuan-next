use crate::object_storage::S3Ref;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;
use zihuan_core::runtime::block_async;
use zihuan_core::error::{Error, Result};

pub struct RustfsNode {
    id: String,
    name: String,
}

impl RustfsNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
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
        Some("RustFS 对象存储配置 - 构建 S3Ref 引用供下游节点上传对象")
    }

    node_input![
        port! { name = "endpoint", ty = String, desc = "对象存储 endpoint" },
        port! { name = "bucket", ty = String, desc = "对象存储 bucket" },
        port! { name = "access_key", ty = String, desc = "对象存储 access key" },
        port! { name = "secret_key", ty = Password, desc = "对象存储 secret key" },
        port! { name = "region", ty = String, desc = "对象存储 region" },
        port! { name = "public_base_url", ty = String, desc = "可选：对象公开访问基地址", optional },
        port! { name = "path_style", ty = Boolean, desc = "可选：是否使用 path-style URL，默认 true", optional },
    ];

    node_output![port! { name = "s3_ref", ty = S3Ref, desc = "对象存储引用" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let endpoint = read_required_string(&inputs, "endpoint")?;
        let bucket = read_required_string(&inputs, "bucket")?;
        let access_key = read_required_string(&inputs, "access_key")?;
        let secret_key = read_required_password(&inputs, "secret_key")?;
        let region = read_required_string(&inputs, "region")?;
        let public_base_url = inputs.get("public_base_url").and_then(|value| match value {
            DataValue::String(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        });
        let path_style = inputs
            .get("path_style")
            .and_then(|value| match value {
                DataValue::Boolean(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(true);

        let s3_ref = Arc::new(S3Ref {
            endpoint,
            bucket,
            access_key,
            secret_key,
            region,
            public_base_url,
            path_style,
        });

        let s3_ref_for_init = Arc::clone(&s3_ref);
        block_async(async move { s3_ref_for_init.ensure_bucket_exists().await })?;

        let outputs = HashMap::from([("s3_ref".to_string(), DataValue::S3Ref(s3_ref))]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

fn read_required_string(inputs: &HashMap<String, DataValue>, key: &str) -> Result<String> {
    let value = inputs
        .get(key)
        .and_then(|value| match value {
            DataValue::String(value) => Some(value.trim().to_string()),
            _ => None,
        })
        .ok_or_else(|| Error::ValidationError(format!("{key} is required")))?;

    if value.is_empty() {
        return Err(Error::ValidationError(format!("{key} must not be empty")));
    }

    Ok(value)
}

fn read_required_password(inputs: &HashMap<String, DataValue>, key: &str) -> Result<String> {
    let value = inputs
        .get(key)
        .and_then(|value| match value {
            DataValue::Password(value) => Some(value.trim().to_string()),
            _ => None,
        })
        .ok_or_else(|| Error::ValidationError(format!("{key} is required")))?;

    if value.is_empty() {
        return Err(Error::ValidationError(format!("{key} must not be empty")));
    }

    Ok(value)
}
