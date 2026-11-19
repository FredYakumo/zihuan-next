use crate::config::{ConfigCenter, ConfigKind, StoredConfigRecord};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanerMember {
    pub role_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub workspace_path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanerConfig {
    #[serde(default)]
    pub config_id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub llm_ref_id: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub members: Vec<PlanerMember>,
}
pub fn load_planers() -> Result<Vec<PlanerConfig>> {
    ConfigCenter::shared()
        .load_root()?
        .configs
        .planers
        .into_iter()
        .map(|r| Ok(serde_json::from_value(r.spec)?))
        .collect()
}
pub fn save_planer(config: PlanerConfig) -> Result<()> {
    ConfigCenter::shared().update_root(|root| {
        if config.config_id.trim().is_empty() || config.name.trim().is_empty() {
            return Err(crate::string_error!("Planer ID 和名称不能为空"));
        }
        if !root.configs.llm_refs.iter().any(|r| {
            r.config_id == config.llm_ref_id
                && r.enabled
                && matches!(r.spec["type"].as_str(), Some("chat_llm_api" | "chat_llm_local"))
        }) {
            return Err(crate::string_error!("请选择有效且启用的模型"));
        }
        let others = root
            .configs
            .planers
            .iter()
            .filter(|r| r.config_id != config.config_id)
            .map(|r| serde_json::from_value::<PlanerConfig>(r.spec.clone()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut seen = HashSet::new();
        for member in &config.members {
            if !seen.insert(&member.role_id) {
                return Err(crate::string_error!("成员不能重复"));
            }
            if others.iter().any(|p| p.members.iter().any(|m| m.role_id == member.role_id)) {
                return Err(crate::string_error!("Role 已归属其他 Planer"));
            }
            let role = root
                .configs
                .agents
                .iter()
                .find(|r| r.config_id == member.role_id)
                .ok_or_else(|| crate::string_error!("Role 不存在"))?;
            match role.kind {
                ConfigKind::ServiceWorkspace => {
                    let path = member.workspace_path.as_deref().unwrap_or("").trim();
                    if path.is_empty() || !std::path::Path::new(path).is_absolute() {
                        return Err(crate::string_error!("Workspace 成员需要绝对工作目录"));
                    }
                }
                ConfigKind::ServiceQqChat => {}
                _ => return Err(crate::string_error!("仅支持 IMS 和 Workspace Role")),
            }
        }
        let record = StoredConfigRecord {
            config_id: config.config_id.clone(),
            kind: ConfigKind::Planer,
            name: config.name.clone(),
            enabled: config.enabled,
            updated_at: config.updated_at.clone(),
            spec: serde_json::to_value(&config)?,
        };
        if let Some(old) = root.configs.planers.iter_mut().find(|r| r.config_id == config.config_id)
        {
            *old = record;
        } else {
            root.configs.planers.push(record);
        }
        Ok(())
    })
}
pub fn delete_planer(id: &str) -> Result<bool> {
    ConfigCenter::shared().update_root(|root| {
        let before = root.configs.planers.len();
        root.configs.planers.retain(|r| r.config_id != id);
        Ok(before != root.configs.planers.len())
    })
}
