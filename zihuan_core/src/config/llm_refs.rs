use log::info;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::{ConfigCategory, ConfigCenter, ConfigKind, ConfigRecord, StoredConfigRecord};
use crate::error::Result;
use crate::model_inference::model_config::{LlmApiStyle, LlmServiceConfig, ModelRefSpec};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRefConfig {
    #[serde(default, skip_serializing)]
    pub id: String,
    #[serde(default)]
    pub config_id: String,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub model: ModelRefSpec,
    #[serde(default)]
    pub updated_at: String,
}

impl LlmRefConfig {
    pub fn canonical_config_id(&self) -> &str {
        if self.config_id.trim().is_empty() {
            &self.id
        } else {
            &self.config_id
        }
    }

    pub fn chat_llm(&self) -> Option<&LlmServiceConfig> {
        match &self.model {
            ModelRefSpec::ChatLlm { llm } => Some(llm),
            ModelRefSpec::TextEmbeddingLocal { .. } => None,
        }
    }
}

impl ConfigRecord for LlmRefConfig {
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
        ConfigKind::LlmRef
    }

    fn validate(&self) -> Result<()> {
        if self.canonical_config_id().trim().is_empty() {
            return Err(crate::string_error!("llm_ref config_id must not be empty"));
        }
        if self.name.trim().is_empty() {
            return Err(crate::string_error!("llm_ref name must not be empty"));
        }
        match &self.model {
            ModelRefSpec::ChatLlm { llm } => {
                if llm.model_name.trim().is_empty() {
                    return Err(crate::string_error!("chat_llm model_name must not be empty"));
                }
                if !matches!(llm.api_style, LlmApiStyle::CandleGguf | LlmApiStyle::CandleHf)
                    && llm.api_endpoint.trim().is_empty()
                {
                    return Err(crate::string_error!("chat_llm api_endpoint must not be empty"));
                }
            }
            ModelRefSpec::TextEmbeddingLocal { model_name } if model_name.trim().is_empty() => {
                return Err(crate::string_error!("text_embedding_local model_name must not be empty"));
            }
            ModelRefSpec::TextEmbeddingLocal { .. } => {}
        }
        Ok(())
    }

    fn redacted_summary(&self) -> Value {
        let model = match &self.model {
            ModelRefSpec::ChatLlm { llm } => json!({
                "type": "chat_llm",
                "llm": {
                    "model_name": llm.model_name,
                    "api_endpoint": llm.api_endpoint,
                    "api_style": llm.api_style,
                }
            }),
            ModelRefSpec::TextEmbeddingLocal { model_name } => json!({
                "type": "text_embedding_local",
                "model_name": model_name,
            }),
        };
        json!({
            "config_id": self.canonical_config_id(),
            "kind": self.kind(),
            "name": self.name,
            "enabled": self.enabled,
            "model": model,
        })
    }
}

pub fn load_llm_refs() -> Result<Vec<LlmRefConfig>> {
    let llm_refs = ConfigCenter::shared()
        .list_configs(ConfigCategory::LlmRef)?
        .into_iter()
        .map(llm_ref_from_record)
        .collect::<Result<Vec<_>>>()?;
    for llm_ref in &llm_refs {
        info!(
            "[config_center] loaded llm_ref config_id={} name='{}'",
            llm_ref.canonical_config_id(),
            llm_ref.name
        );
    }
    Ok(llm_refs)
}

pub fn save_llm_refs(llm_refs: Vec<LlmRefConfig>) -> Result<()> {
    let center = ConfigCenter::shared();
    let existing_ids = center
        .list_configs(ConfigCategory::LlmRef)?
        .into_iter()
        .map(|record| record.config_id)
        .collect::<std::collections::HashSet<_>>();
    let mut incoming_ids = std::collections::HashSet::new();

    for llm_ref in llm_refs {
        let llm_ref = normalize_identity(llm_ref, center.new_config_id());
        let record = llm_ref_to_record(&llm_ref)?;
        incoming_ids.insert(record.config_id.clone());
        center.upsert_config(record)?;
    }
    for config_id in existing_ids {
        if !incoming_ids.contains(&config_id) {
            let _ = center.delete_config(ConfigCategory::LlmRef, &config_id)?;
        }
    }
    Ok(())
}

fn normalize_identity(mut llm_ref: LlmRefConfig, fallback_id: String) -> LlmRefConfig {
    let canonical = if llm_ref.config_id.trim().is_empty() {
        if llm_ref.id.trim().is_empty() { fallback_id } else { llm_ref.id.clone() }
    } else {
        llm_ref.config_id.clone()
    };
    llm_ref.id = canonical.clone();
    llm_ref.config_id = canonical;
    llm_ref
}

fn llm_ref_to_record(llm_ref: &LlmRefConfig) -> Result<StoredConfigRecord> {
    llm_ref.validate()?;
    Ok(StoredConfigRecord {
        config_id: llm_ref.canonical_config_id().to_string(),
        kind: ConfigKind::LlmRef,
        name: llm_ref.name.clone(),
        enabled: llm_ref.enabled,
        updated_at: llm_ref.updated_at.clone(),
        spec: serde_json::to_value(&llm_ref.model)?,
    })
}

fn llm_ref_from_record(record: StoredConfigRecord) -> Result<LlmRefConfig> {
    if record.kind != ConfigKind::LlmRef {
        return Err(crate::string_error!("config '{}' is not an llm_ref config", record.config_id));
    }
    Ok(LlmRefConfig {
        id: record.config_id.clone(),
        config_id: record.config_id,
        name: record.name,
        enabled: record.enabled,
        model: model_ref_spec_from_value(record.spec)?,
        updated_at: record.updated_at,
    })
}

fn model_ref_spec_from_value(value: Value) -> Result<ModelRefSpec> {
    if value.as_object().and_then(|object| object.get("type")).and_then(Value::as_str).is_some() {
        return Ok(serde_json::from_value(value)?);
    }
    Ok(ModelRefSpec::ChatLlm { llm: serde_json::from_value(value)? })
}
