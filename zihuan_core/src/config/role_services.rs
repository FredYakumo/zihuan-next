use log::{info, warn};
use serde_json::{json, Map, Value};

use crate::agent::service_config::{RoleServiceConfig, RoleServiceType};
use crate::config::{ConfigCategory, ConfigCenter, ConfigKind, ConfigRecord, StoredConfigRecord};
use crate::error::Result;

impl ConfigRecord for RoleServiceConfig {
    fn config_id(&self) -> &str {
        self.canonical_config_id()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn updated_at(&self) -> &str {
        &self.updated_at
    }

    fn kind(&self) -> ConfigKind {
        match self.role_service_type {
            RoleServiceType::QqChat(_) => ConfigKind::ServiceQqChat,
            RoleServiceType::Workspace(_) => ConfigKind::ServiceWorkspace,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.canonical_config_id().trim().is_empty() {
            return Err(crate::string_error!("agent config_id must not be empty"));
        }
        if self.name.trim().is_empty() {
            return Err(crate::string_error!("agent name must not be empty"));
        }
        Ok(())
    }

    fn redacted_summary(&self) -> Value {
        json!({
            "config_id": self.canonical_config_id(),
            "kind": self.kind(),
            "name": self.name,
            "enabled": self.enabled,
        })
    }
}

pub fn load_role_services() -> Result<Vec<RoleServiceConfig>> {
    let agents = ConfigCenter::shared()
        .list_configs(ConfigCategory::Service)?
        .into_iter()
        .map(agent_from_record)
        .collect::<Result<Vec<_>>>()?;
    for agent in &agents {
        info!(
            "[config_center] loaded agent config_id={} kind={:?} name='{}'",
            agent.canonical_config_id(),
            agent.kind(),
            agent.name
        );
    }
    Ok(agents)
}

pub fn save_role_services(agents: Vec<RoleServiceConfig>) -> Result<()> {
    let center = ConfigCenter::shared();
    let existing_ids = center
        .list_configs(ConfigCategory::Service)?
        .into_iter()
        .map(|record| record.config_id)
        .collect::<std::collections::HashSet<_>>();
    let mut incoming_ids = std::collections::HashSet::new();

    for agent in agents {
        let agent = normalize_identity(agent, center.new_config_id());
        let record = agent_to_record(&agent)?;
        incoming_ids.insert(record.config_id.clone());
        center.upsert_config(record)?;
    }
    for config_id in existing_ids {
        if !incoming_ids.contains(&config_id) {
            let _ = center.delete_config(ConfigCategory::Service, &config_id)?;
        }
    }
    Ok(())
}

fn normalize_identity(mut agent: RoleServiceConfig, fallback_id: String) -> RoleServiceConfig {
    let canonical = if agent.config_id.trim().is_empty() {
        if agent.id.trim().is_empty() {
            fallback_id
        } else {
            agent.id.clone()
        }
    } else {
        agent.config_id.clone()
    };
    agent.id = canonical.clone();
    agent.config_id = canonical;
    agent
}

fn agent_to_record(agent: &RoleServiceConfig) -> Result<StoredConfigRecord> {
    agent.validate()?;
    let mut spec = Map::new();
    spec.insert("role_service_type".to_string(), serde_json::to_value(&agent.role_service_type)?);
    spec.insert("auto_start".to_string(), Value::Bool(agent.auto_start));
    spec.insert("is_default".to_string(), Value::Bool(agent.is_default));
    spec.insert("tools".to_string(), serde_json::to_value(&agent.tools)?);
    if let Some(avatar_url) = &agent.avatar_url {
        spec.insert("avatar_url".to_string(), Value::String(avatar_url.clone()));
    }
    Ok(StoredConfigRecord {
        config_id: agent.canonical_config_id().to_string(),
        kind: agent.kind(),
        name: agent.name.clone(),
        enabled: agent.enabled,
        updated_at: agent.updated_at.clone(),
        spec: Value::Object(spec),
    })
}

fn agent_from_record(record: StoredConfigRecord) -> Result<RoleServiceConfig> {
    if record.kind.category() != ConfigCategory::Service {
        return Err(crate::string_error!("config '{}' is not an agent config", record.config_id));
    }
    let spec = record.spec.as_object().ok_or_else(|| {
        crate::string_error!("agent config '{}' spec must be an object", record.config_id)
    })?;
    let mut role_service_type =
        serde_json::from_value(spec.get("role_service_type").cloned().unwrap_or(Value::Null))?;
    migrate_legacy_qq_rdb_id(&record.config_id, &mut role_service_type);

    Ok(RoleServiceConfig {
        id: record.config_id.clone(),
        config_id: record.config_id.clone(),
        name: record.name,
        role_service_type,
        enabled: record.enabled,
        auto_start: spec.get("auto_start").and_then(Value::as_bool).unwrap_or(false),
        is_default: spec.get("is_default").and_then(Value::as_bool).unwrap_or(false),
        updated_at: record.updated_at,
        tools: serde_json::from_value(
            spec.get("tools").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
        )?,
        avatar_url: spec
            .get("avatar_url")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty()),
    })
}

fn migrate_legacy_qq_rdb_id(config_id: &str, role_service_type: &mut RoleServiceType) {
    let RoleServiceType::QqChat(config) = role_service_type else {
        return;
    };
    let rdb_id = non_empty_id(config.rdb_id.as_deref());
    let mysql_connection_id = non_empty_id(config.mysql_connection_id.as_deref());
    let task_db_connection_id = non_empty_id(config.task_db_connection_id.as_deref());
    if rdb_id.is_some() {
        return;
    }
    if let (Some(mysql_id), Some(task_id)) = (&mysql_connection_id, &task_db_connection_id) {
        if mysql_id != task_id {
            warn!(
                "[config_center] qq_chat agent '{}' has conflicting legacy mysql_connection_id='{}' and task_db_connection_id='{}'; using mysql_connection_id",
                config_id, mysql_id, task_id
            );
        }
    }
    config.rdb_id = mysql_connection_id.or(task_db_connection_id);
}

fn non_empty_id(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned)
}
