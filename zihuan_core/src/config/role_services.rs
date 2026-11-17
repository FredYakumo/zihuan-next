use log::info;
use serde_json::{json, Map, Value};

use crate::config::{ConfigCategory, ConfigCenter, ConfigKind, ConfigRecord, StoredConfigRecord};
use crate::error::Result;
use crate::role::service_config::{RoleServiceConfig, RoleServiceKind, RoleServiceType};

pub fn role_service_config_kind(role_service_type: &RoleServiceType) -> ConfigKind {
    match role_service_type.kind {
        RoleServiceKind::QqChat => ConfigKind::ServiceQqChat,
        RoleServiceKind::Workspace => ConfigKind::ServiceWorkspace,
    }
}

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
        role_service_config_kind(&self.role_service_type)
    }

    fn validate(&self) -> Result<()> {
        if self.canonical_config_id().trim().is_empty() {
            return Err(crate::string_error!("role service config_id must not be empty"));
        }
        if self.name.trim().is_empty() {
            return Err(crate::string_error!("role service name must not be empty"));
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
    let role_services = ConfigCenter::shared()
        .list_configs(ConfigCategory::Service)?
        .into_iter()
        .map(role_service_from_record)
        .collect::<Result<Vec<_>>>()?;
    for role_service in &role_services {
        info!(
            "[config_center] loaded role service config_id={} kind={:?} name='{}'",
            role_service.canonical_config_id(),
            role_service.kind(),
            role_service.name
        );
    }
    Ok(role_services)
}

pub fn save_role_services(role_services: Vec<RoleServiceConfig>) -> Result<()> {
    let center = ConfigCenter::shared();
    let records = role_services
        .into_iter()
        .map(|role| role_service_to_record(&normalize_identity(role, center.new_config_id())))
        .collect::<Result<Vec<_>>>()?;
    center.update_root(|root| {
        // Check and replace atomically with Planer edits, including bulk configuration writes.
        let incoming_ids = records
            .iter()
            .map(|record| record.config_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        for planer in &root.configs.planers {
            let config: super::planers::PlanerConfig =
                serde_json::from_value(planer.spec.clone())?;
            if config
                .members
                .iter()
                .any(|member| !incoming_ids.contains(member.role_id.as_str()))
            {
                return Err(crate::string_error!("Role 仍被 Planer 引用，请先移除成员"));
            }
        }
        root.configs.agents = records;
        Ok(())
    })
}

fn normalize_identity(
    mut role_service: RoleServiceConfig,
    fallback_id: String,
) -> RoleServiceConfig {
    let canonical = if role_service.config_id.trim().is_empty() {
        if role_service.id.trim().is_empty() {
            fallback_id
        } else {
            role_service.id.clone()
        }
    } else {
        role_service.config_id.clone()
    };
    role_service.id = canonical.clone();
    role_service.config_id = canonical;
    role_service
}

fn role_service_to_record(role_service: &RoleServiceConfig) -> Result<StoredConfigRecord> {
    role_service.validate()?;
    let mut spec = Map::new();
    spec.insert(
        "role_service_type".to_string(),
        serde_json::to_value(&role_service.role_service_type)?,
    );
    spec.insert("auto_start".to_string(), Value::Bool(role_service.auto_start));
    spec.insert("is_default".to_string(), Value::Bool(role_service.is_default));
    spec.insert("tools".to_string(), serde_json::to_value(&role_service.tools)?);
    if let Some(avatar_url) = &role_service.avatar_url {
        spec.insert("avatar_url".to_string(), Value::String(avatar_url.clone()));
    }
    Ok(StoredConfigRecord {
        config_id: role_service.canonical_config_id().to_string(),
        kind: role_service.kind(),
        name: role_service.name.clone(),
        enabled: role_service.enabled,
        updated_at: role_service.updated_at.clone(),
        spec: Value::Object(spec),
    })
}

fn role_service_from_record(record: StoredConfigRecord) -> Result<RoleServiceConfig> {
    if record.kind.category() != ConfigCategory::Service {
        return Err(crate::string_error!(
            "config '{}' is not a role service config",
            record.config_id
        ));
    }
    let spec = record.spec.as_object().ok_or_else(|| {
        crate::string_error!("role service config '{}' spec must be an object", record.config_id)
    })?;
    let role_service_type =
        serde_json::from_value(spec.get("role_service_type").cloned().unwrap_or(Value::Null))?;

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
