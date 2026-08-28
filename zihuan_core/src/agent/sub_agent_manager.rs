use std::fs;
use std::path::{Path, PathBuf};

use crate::agent::sub_agent::SubAgentDefinition;
use crate::error::{Error, Result};
use crate::graph::function_graph::FunctionPortDef;
use crate::graph::DataType;
use crate::system_config::application_data_dir;

const DEFAULT_MEMORY_ID: &str = "memory";
const DEFAULT_DREAM_ID: &str = "dream";

pub fn subagent_dir() -> PathBuf {
    application_data_dir().join("sub_agents")
}

/// Creates the built-in definitions that are absent at application startup.
/// Existing files are intentionally left untouched.
pub fn ensure_default_subagents() -> Result<()> {
    ensure_default_subagents_at(&subagent_dir())
}

fn ensure_default_subagents_at(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory).map_err(|error| {
        Error::ValidationError(format!("failed to create subagent directory: {error}"))
    })?;

    for definition in [default_memory_definition(), default_dream_definition()] {
        let path = directory.join(format!("{}.yaml", definition.id));
        if path.exists() {
            continue;
        }
        let yaml = serde_yaml::to_string(&definition).map_err(|error| {
            Error::ValidationError(format!("failed to serialize default subagent '{}': {error}", definition.id))
        })?;
        fs::write(&path, yaml).map_err(|error| {
            Error::ValidationError(format!("failed to write default subagent '{}': {error}", path.display()))
        })?;
    }
    Ok(())
}

fn default_memory_definition() -> SubAgentDefinition {
    SubAgentDefinition {
        id: DEFAULT_MEMORY_ID.to_string(),
        name: "Memory".to_string(),
        inputs: vec![FunctionPortDef {
            name: "content".to_string(),
            data_type: DataType::String,
            description: "Memory request or chat context".to_string(),
            required: true,
        }],
        outputs: vec![FunctionPortDef {
            name: "result".to_string(),
            data_type: DataType::String,
            description: "Memory result".to_string(),
            required: true,
        }],
        system_prompt: "You manage durable role memory. Return JSON with a result field only.".to_string(),
        tool_ids: vec!["search_memory".to_string(), "update_memory".to_string(), "list_memory_keys".to_string()],
    }
}

fn default_dream_definition() -> SubAgentDefinition {
    SubAgentDefinition {
        id: DEFAULT_DREAM_ID.to_string(),
        name: "Dream".to_string(),
        inputs: vec![FunctionPortDef {
            name: "transcript".to_string(),
            data_type: DataType::String,
            description: "Conversation transcript".to_string(),
            required: true,
        }],
        outputs: vec![FunctionPortDef {
            name: "memory".to_string(),
            data_type: DataType::String,
            description: "Consolidated memory".to_string(),
            required: true,
        }],
        system_prompt: "Consolidate durable facts from the transcript. Return JSON with a memory field only.".to_string(),
        tool_ids: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn creates_missing_defaults_without_overwriting_existing_files() {
        let directory = std::env::temp_dir().join(format!("zihuan-subagent-manager-{}", uuid::Uuid::new_v4()));
        ensure_default_subagents_at(&directory).unwrap();
        let memory = directory.join("memory.yaml");
        assert!(memory.exists());
        assert!(directory.join("dream.yaml").exists());

        fs::write(&memory, "user-edited: true\n").unwrap();
        ensure_default_subagents_at(&directory).unwrap();
        assert_eq!(fs::read_to_string(&memory).unwrap(), "user-edited: true\n");

        fs::remove_file(&memory).unwrap();
        ensure_default_subagents_at(&directory).unwrap();
        let restored: SubAgentDefinition = serde_yaml::from_str(&fs::read_to_string(&memory).unwrap()).unwrap();
        assert_eq!(restored.id, DEFAULT_MEMORY_ID);
        fs::remove_dir_all(directory).unwrap();
    }
}
