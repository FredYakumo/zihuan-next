use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::agent::tools::Tool;
use crate::error::{Error, Result};
use crate::model_inference::llm::tooling::FunctionTool;
use crate::storage::{MemoryCategory, PersonMemory};
use crate::utils::json_utils::{optional_string_argument, required_string_argument};

use super::category_argument;
use super::function_spec::MemoryFunctionToolSpec;
use super::resources::MemoryAgentResources;

/// Creates or corrects one memory (`update_memory`).
pub struct RememberMemoryTool {
    resources: MemoryAgentResources,
}

impl RememberMemoryTool {
    pub fn new(resources: MemoryAgentResources) -> Self {
        Self { resources }
    }
}

const UPDATE_MEMORY_TOOL_DESC: &'static str = r#"Create or correct a memory in an explicit category.
Concept and behavioral use a key.
Person has no key and requires immutable qq_id and structured profile;never merge different QQ IDs.
Behavioral content must be short.
Source is ROLE_SELF for the Role Service's own independent reflection (level 2),planer for Planer tasks (level 1), or user with the verified triggering sender_id and access_level.
User corrections must use user source.
Existing scope and access level are preserved"#;

impl Tool for RememberMemoryTool {
    fn execution_resource(&self, _: &Value) -> crate::agent::tools::ToolExecutionResource {
        crate::agent::tools::ToolExecutionResource::Exclusive
    }
    fn returns_failure_details_to_agent(&self) -> bool {
        true
    }
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new(
            "update_memory",
            UPDATE_MEMORY_TOOL_DESC,
            json!({"type":"object","properties":{
            "category":{"type":"string","enum":["concept","person","behavioral"]},"key":{"type":"string"},"content":{"type":"string"},
            "source":{"type":"object","properties":{"kind":{"type":"string","enum":["ROLE_SELF","user","planer"]},"sender_id":{"type":"string"},"access_level":{"type":"integer","minimum":0}},"required":["kind"],"additionalProperties":false},
            "person":{"type":"object","properties":{"qq_id":{"type":"string"},"name":{"type":"string"},"preferred_name":{"type":"string"},"aliases":{"type":"array","items":{"type":"string"}},"impression":{"type":"string"},"guideline":{"type":"string"}},"additionalProperties":false},
            "starts_at":{"type":"string"},"scope":{"type":"string","enum":["current","global","group","sender"]}
        },"required":["category","content","source"],"additionalProperties":false}),
        ))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let _write_guard = crate::storage::MEMORY_MUTATION_LOCK
            .lock()
            .map_err(|_| crate::string_error!("memory mutation lock poisoned"))?;
        let category = category_argument(arguments)?;
        let source = arguments
            .get("source")
            .ok_or_else(|| Error::ValidationError("source required".to_string()))?;
        let source_kind = required_string_argument(source, "kind")?;
        let level = match source_kind.as_str() {
            "ROLE_SELF" => crate::storage::user_permission::ROLE_SELF_ACCESS_LEVEL,
            "planer" => crate::storage::user_permission::PLANER_ACCESS_LEVEL,
            "user" => {
                let sender = required_string_argument(source, "sender_id")?;
                if self.resources.access.sender_id.as_deref() != Some(&sender) {
                    return Err(Error::ValidationError(
                        "source sender must be the authenticated triggering user".to_string(),
                    ));
                }
                let actual = crate::storage::user_permission::memory_source_level(
                    &self.resources.access,
                    &sender,
                )?;
                if source.get("access_level").and_then(Value::as_u64) != Some(u64::from(actual)) {
                    return Err(Error::ValidationError(format!(
                        "source access_level does not match authenticated level {actual}"
                    )));
                }
                actual
            }
            _ => {
                return Err(Error::ValidationError(
                    "source kind must be ROLE_SELF, user or planer".to_string(),
                ))
            }
        };
        let content = required_string_argument(arguments, "content")?;
        let person_patch = arguments.get("person");
        let mut person: PersonMemory =
            serde_json::from_value(person_patch.cloned().unwrap_or_else(|| json!({})))?;
        let key = if category == MemoryCategory::Person {
            // IM ids are opaque: QQ uses digits, WeChat uses wxid / `xxx@chatroom`.
            if person.qq_id.is_empty()
                || !person.qq_id.chars().all(crate::utils::string_utils::is_im_id_char)
            {
                return Err(Error::ValidationError(
                    "person.qq_id must be a non-empty person id".to_string(),
                ));
            }
            String::new()
        } else if category == MemoryCategory::Behavioral {
            optional_string_argument(arguments, "key").unwrap_or_default()
        } else {
            required_string_argument(arguments, "key")?
        };
        let scope = arguments.get("scope").and_then(Value::as_str).unwrap_or("current");
        let sender_id_list: Vec<String> =
            if category != MemoryCategory::Person && matches!(scope, "current" | "sender") {
                self.resources.access.sender_id.clone().into_iter().collect()
            } else {
                vec![]
            };
        let group_id_list: Vec<String> =
            if category != MemoryCategory::Person && matches!(scope, "current" | "group") {
                self.resources.access.group_id.clone().into_iter().collect()
            } else {
                vec![]
            };
        if !matches!(scope, "current" | "sender" | "group" | "global") {
            return Err(Error::ValidationError("invalid scope".to_string()));
        }
        let key = if category == MemoryCategory::Behavioral && key.is_empty() {
            crate::storage::memory::behavioral_key(&content, &sender_id_list, &group_id_list)
        } else {
            key
        };
        let backend = &self.resources.memory_backend;
        let existing = match category {
            MemoryCategory::Person => {
                backend.get_person(&person.qq_id)?.map(|record| record.into_record())
            }
            MemoryCategory::Concept => backend
                .get_concept(&key, &sender_id_list, &group_id_list)?
                .map(|record| record.into_record()),
            MemoryCategory::Behavioral => backend
                .get_behavioral(&key, &sender_id_list, &group_id_list)?
                .map(|record| record.into_record()),
        };
        if let Some(existing) = &existing {
            if source_kind == "user" && level > existing.attributes.access_level {
                return Err(Error::ValidationError(
                    "user access level cannot modify this memory".to_string(),
                ));
            }
            if category == MemoryCategory::Person {
                let mut merged = serde_json::to_value(&existing.attributes.person)?;
                if let Some(patch) = person_patch.and_then(Value::as_object) {
                    merged.as_object_mut().unwrap().extend(patch.clone());
                }
                person = serde_json::from_value(merged)?;
            }
        }
        if category == MemoryCategory::Person && person.impression.is_empty() {
            person.impression = content.clone();
        }
        let starts_at = optional_string_argument(arguments, "starts_at")
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        chrono::DateTime::parse_from_rfc3339(&starts_at)
            .map_err(|_| Error::ValidationError("invalid starts_at".to_string()))?;
        let value = if category == MemoryCategory::Person {
            serde_json::to_string(&person)?
        } else {
            content
        };
        let access_level =
            existing.as_ref().map(|record| record.attributes.access_level).unwrap_or(level);
        let model = self.resources.embedding_model.as_deref();
        let record = match category {
            MemoryCategory::Person => backend
                .save_person(
                    model,
                    &crate::storage::PersonMemoryWrite { profile: person, access_level },
                )?
                .into_record(),
            MemoryCategory::Concept => backend
                .save_concept(
                    model,
                    &crate::storage::ConceptMemoryWrite {
                        key,
                        content: value,
                        access_level,
                        starts_at: Some(starts_at),
                        sender_id_list,
                        group_id_list,
                    },
                )?
                .into_record(),
            MemoryCategory::Behavioral => backend
                .save_behavioral(
                    model,
                    &crate::storage::BehavioralMemoryWrite {
                        key,
                        content: value,
                        access_level,
                        expires_at: Some((Utc::now() + Duration::days(2)).to_rfc3339()),
                        sender_id_list,
                        group_id_list,
                    },
                )?
                .into_record(),
        };
        Ok(json!({"ok":true,"item":record}).to_string())
    }
}
