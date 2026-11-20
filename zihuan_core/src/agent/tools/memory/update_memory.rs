use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::agent::tools::Tool;
use crate::error::{Error, Result};
use crate::model_inference::llm::tooling::FunctionTool;
use crate::storage::{MemoryCategory, MemoryRecord, MemoryWriteSource, PersonMemory};
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

const UPDATE_MEMORY_TOOL_DESC: &str = r#"Create or correct a memory in an explicit category. `key` is required for concept and behavioral."#;

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
            "person":{"type":"object","properties":{"qq_id":{"type":"string"},"name":{"type":"string"},"preferred_name":{"type":"string"},"aliases":{"type":"array","items":{"type":"string"}},"impression":{"type":"string"},"guideline":{"type":"string"}},"additionalProperties":false},
            "starts_at":{"type":"string"},"scope":{"type":"string","enum":["current","global","group","sender"]}
        },"required":["category","content"],"additionalProperties":false}),
        ))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let _write_guard = crate::storage::MEMORY_MUTATION_LOCK
            .lock()
            .map_err(|_| crate::string_error!("memory mutation lock poisoned"))?;
        let category = category_argument(arguments)?;
        let level = match self.resources.access.write_source {
            MemoryWriteSource::RoleSelf => crate::storage::user_permission::ROLE_SELF_ACCESS_LEVEL,
            MemoryWriteSource::Planer => crate::storage::user_permission::PLANER_ACCESS_LEVEL,
            MemoryWriteSource::User => match self.resources.access.sender_id.as_deref() {
                Some(sender) => crate::storage::user_permission::memory_source_level(
                    &self.resources.access,
                    sender,
                )?,
                // Workspace has no authenticated IM sender and cannot claim an elevated level.
                None => crate::storage::user_permission::DEFAULT_ACCESS_LEVEL,
            },
            MemoryWriteSource::Disabled => {
                return Ok(json!({"ok":false,"error":"没有权限"}).to_string());
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
        let backend = &self.resources.memory_backend;
        let existing = match category {
            MemoryCategory::Person => {
                backend.get_person(&person.qq_id)?.map(crate::storage::MemoryRecord::Person)
            }
            MemoryCategory::Concept => backend
                .get_concept(&key, &sender_id_list, &group_id_list)?
                .map(crate::storage::MemoryRecord::Concept),
            MemoryCategory::Behavioral => backend
                .get_behavioral(&key, &sender_id_list, &group_id_list)?
                .map(crate::storage::MemoryRecord::Behavioral),
        };
        if let Some(existing) = &existing {
            if level > existing.access_level() {
                return Ok(json!({"ok":false,"error":"没有权限"}).to_string());
            }
            if category == MemoryCategory::Person {
                let MemoryRecord::Person(existing) = existing else {
                    unreachable!("person lookup returns a person record")
                };
                let mut merged = serde_json::to_value(&existing.profile)?;
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
        let access_level = existing.as_ref().map(|record| record.access_level()).unwrap_or(level);
        let model = self.resources.embedding_model.as_deref();
        let record = match category {
            MemoryCategory::Person => crate::storage::MemoryRecord::Person(backend.save_person(
                model,
                &crate::storage::PersonMemoryWrite { profile: person, access_level },
            )?),
            MemoryCategory::Concept => {
                crate::storage::MemoryRecord::Concept(backend.save_concept(
                    model,
                    &crate::storage::ConceptMemoryWrite {
                        key,
                        content: value,
                        access_level,
                        starts_at: Some(starts_at),
                        sender_id_list,
                        group_id_list,
                    },
                )?)
            }
            MemoryCategory::Behavioral => {
                crate::storage::MemoryRecord::Behavioral(backend.save_behavioral(
                    model,
                    &crate::storage::BehavioralMemoryWrite {
                        key,
                        content: value,
                        access_level,
                        expires_at: Some((Utc::now() + Duration::days(2)).to_rfc3339()),
                        sender_id_list,
                        group_id_list,
                    },
                )?)
            }
        };
        Ok(json!({"ok":true,"item":record}).to_string())
    }
}
