//! Shared naming and persistent volumes for newly generated Compose projects.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DockerSettings {
    pub stack_name: String,
    pub persistence_name: String,
}

impl Default for DockerSettings {
    fn default() -> Self {
        Self {
            stack_name: "zihuan".into(),
            persistence_name: "zihuan-data".into(),
        }
    }
}

impl DockerSettings {
    pub fn validate(&self) -> Result<(), String> {
        for (label, name) in [
            ("组件安装 stack 名称", &self.stack_name),
            ("组件数据持久化名称", &self.persistence_name),
        ] {
            if name.is_empty()
                || name.len() > 63
                || !name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                || !name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
            {
                return Err(format!("{label}需以小写字母或数字开头，仅含小写字母、数字、连字符和下划线，最多 63 个字符"));
            }
        }
        Ok(())
    }
}

pub fn load() -> Result<DockerSettings, String> {
    crate::config::read_yaml(&crate::config::root().join("settings.yaml"))
}

pub fn project(identity: &str) -> Result<String, String> {
    let settings = load()?;
    settings.validate()?;
    Ok(format!("{}-{}", settings.stack_name, identity))
}

/// Keep file mounts (e.g. the hook relay script) and explicit absolute bind mounts.
/// Relative data directories become named volumes, initialized from image contents.
pub fn compose(source: &str, identity: &str) -> Result<String, String> {
    let settings = load()?;
    settings.validate()?;
    let mut document: Value = serde_yaml::from_str(source).map_err(|error| error.to_string())?;
    document["name"] = json!(format!("{}-{identity}", settings.stack_name));
    let mut volumes = serde_json::Map::new();
    if let Some(services) = document["services"].as_object_mut() {
        for (service_name, service) in services {
            if let Some(mounts) = service["volumes"].as_array_mut() {
                for (index, mount) in mounts.iter_mut().enumerate() {
                    let Some(spec) = mount.as_str() else { continue };
                    if spec.as_bytes().get(1) == Some(&b':') {
                        continue;
                    }
                    let Some((source, target)) = spec.split_once(':') else {
                        continue;
                    };
                    // Preserve read-only script/config mounts and absolute host paths.
                    if target.ends_with(":ro") || source.starts_with('/') || source.contains('\\') {
                        continue;
                    }
                    let key = format!("{service_name}-data-{index}");
                    volumes.insert(
                        key.clone(),
                        json!({"name": format!("{}-{identity}-{key}", settings.persistence_name)}),
                    );
                    *mount = json!(format!("{key}:{target}"));
                }
            }
        }
    }
    document["volumes"] = Value::Object(volumes);
    serde_yaml::to_string(&document).map_err(|error| error.to_string())
}
