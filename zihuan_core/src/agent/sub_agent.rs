use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::SharedTool;
use crate::agent::agent::{Agent, AgentContext, AgentDescriptor};
use crate::agent::tools::{Tool, ToolCallingEngine, ToolCallingStopReason};
use crate::error::{Error, Result};
use crate::graph::function_graph::FunctionPortDef;
use crate::graph::util::function::data_value_from_json_with_declared_type;
use crate::graph::DataValue;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::tooling::FunctionTool;
use crate::model_inference::llm::LLMMessage;
use crate::agent::sub_agent_manager::subagent_dir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubAgentDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub inputs: Vec<FunctionPortDef>,
    #[serde(default)]
    pub outputs: Vec<FunctionPortDef>,
    pub system_prompt: String,
    #[serde(default)]
    pub tool_ids: Vec<String>,
}

impl SubAgentDefinition {
    pub fn validate(&self, available_tool_ids: &HashSet<String>) -> Result<()> {
        validate_subagent_id(&self.id)?;
        if self.name.trim().is_empty() { return Err(Error::ValidationError("subagent name must not be empty".to_string())); }
        if available_tool_ids.contains(&self.id) { return Err(Error::ValidationError(format!("subagent id '{}' conflicts with an available tool id", self.id))); }
        validate_ports("input", &self.inputs)?;
        validate_ports("output", &self.outputs)?;
        let mut seen = HashSet::new();
        for tool_id in &self.tool_ids {
            if tool_id.trim().is_empty() || !seen.insert(tool_id) { return Err(Error::ValidationError(format!("subagent '{}' has duplicate or empty tool id", self.id))); }
            if !available_tool_ids.contains(tool_id) { return Err(Error::ValidationError(format!("subagent '{}' is not allowed to use tool '{tool_id}'", self.id))); }
        }
        Ok(())
    }
}

pub fn validate_subagent_id(id: &str) -> Result<()> {
    let mut characters = id.chars();
    let Some(first) = characters.next() else {
        return Err(Error::ValidationError("subagent id must not be empty".to_string()));
    };
    if !first.is_ascii_lowercase() || !characters.all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_') {
        return Err(Error::ValidationError("subagent id must start with a lowercase letter and contain only lowercase letters, numbers, or underscores".to_string()));
    }
    Ok(())
}

fn validate_ports(kind: &str, ports: &[FunctionPortDef]) -> Result<()> {
    let mut names = HashSet::new();
    for port in ports {
        let name = port.name.trim();
        if name.is_empty() || !names.insert(name) { return Err(Error::ValidationError(format!("subagent {kind} ports must have unique non-empty names"))); }
    }
    Ok(())
}

pub fn load_subagent_definition(
    id: &str,
    available_tool_ids: &HashSet<String>,
) -> Result<SubAgentDefinition> {
    validate_subagent_id(id)?;
    let path = subagent_dir().join(format!("{id}.yaml"));
    let content = fs::read_to_string(&path).map_err(|error| {
        Error::ValidationError(format!("failed to read subagent '{}': {error}", path.display()))
    })?;
    let definition: SubAgentDefinition = serde_yaml::from_str(&content).map_err(|error| {
        Error::ValidationError(format!("invalid subagent '{}': {error}", path.display()))
    })?;
    definition.validate(available_tool_ids)?;
    Ok(definition)
}

pub fn save_subagent_definition(
    definition: &SubAgentDefinition,
    available_tool_ids: &HashSet<String>,
) -> Result<()> {
    definition.validate(available_tool_ids)?;
    let directory = subagent_dir();
    fs::create_dir_all(&directory).map_err(|error| {
        Error::ValidationError(format!("failed to create subagent directory: {error}"))
    })?;
    save_subagent_definition_at(&directory.join(format!("{}.yaml", definition.id)), definition)
}

pub fn list_subagent_definitions(
    available_tool_ids: &HashSet<String>,
) -> Result<Vec<SubAgentDefinition>> {
    let directory = subagent_dir();
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut definitions = fs::read_dir(&directory)
        .map_err(|error| {
            Error::ValidationError(format!("failed to read subagent directory: {error}"))
        })?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("yaml"))
        .map(|entry| {
            let path = entry.path();
            let content = fs::read_to_string(&path).map_err(|error| {
                Error::ValidationError(format!("failed to read subagent '{}': {error}", path.display()))
            })?;
            let definition: SubAgentDefinition = serde_yaml::from_str(&content).map_err(|error| {
                Error::ValidationError(format!("invalid subagent '{}': {error}", path.display()))
            })?;
            definition.validate(available_tool_ids)?;
            Ok(definition)
        })
        .collect::<Result<Vec<_>>>()?;
    definitions.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(definitions)
}

pub fn delete_subagent_definition(id: &str) -> Result<()> {
    validate_subagent_id(id)?;
    let path = subagent_dir().join(format!("{id}.yaml"));
    if !path.exists() {
        return Err(Error::ValidationError(format!("subagent '{id}' not found")));
    }
    fs::remove_file(&path).map_err(|error| {
        Error::ValidationError(format!("failed to delete subagent '{}': {error}", path.display()))
    })
}

fn save_subagent_definition_at(path: &Path, definition: &SubAgentDefinition) -> Result<()> {
    let yaml = serde_yaml::to_string(definition).map_err(|error| {
        Error::ValidationError(format!("failed to serialize subagent '{}': {error}", definition.id))
    })?;
    fs::write(path, yaml).map_err(|error| {
        Error::ValidationError(format!("failed to write subagent '{}': {error}", path.display()))
    })
}


pub struct SubAgent {
    definition: SubAgentDefinition,
    llm: Arc<dyn LLMBase>,
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl SubAgent {
    pub fn new(definition: SubAgentDefinition, llm: Arc<dyn LLMBase>, tools: HashMap<String, Arc<dyn Tool>>) -> Result<Self> {
        definition.validate(&tools.keys().cloned().collect())?;
        Ok(Self { definition, llm, tools })
    }
    pub fn definition(&self) -> &SubAgentDefinition { &self.definition }
}

#[async_trait]
impl Agent for SubAgent {
    type Input = HashMap<String, DataValue>;
    type Output = HashMap<String, DataValue>;

    fn descriptor(&self) -> AgentDescriptor { AgentDescriptor::new(Box::leak(self.definition.id.clone().into_boxed_str()), Box::leak(self.definition.name.clone().into_boxed_str()), vec!["subagent"]) }

    async fn run(&self, _context: AgentContext, input: Self::Input) -> Result<Self::Output> {
        for port in &self.definition.inputs {
            let value = input.get(&port.name);
            if port.required && value.is_none() { return Err(Error::ValidationError(format!("subagent '{}' missing required input '{}'", self.definition.id, port.name))); }
            if let Some(value) = value.filter(|value| !port.data_type.is_compatible_with(&value.data_type())) { return Err(Error::ValidationError(format!("subagent '{}' input '{}' expected {}, got {}", self.definition.id, port.name, port.data_type, value.data_type()))); }
        }
        let input_json = input.into_iter().map(|(key, value)| (key, value.to_json())).collect::<Map<_, _>>();
        let mut engine = ToolCallingEngine::new(Arc::clone(&self.llm));
        for tool_id in &self.definition.tool_ids { engine.add_tool(SharedTool::new(Arc::clone(self.tools.get(tool_id).expect("validated tool id")))); }
        let (messages, stop_reason) = engine.run(vec![LLMMessage::system(self.definition.system_prompt.clone()), LLMMessage::user(format!("Input:\n{}\n\nReturn only a JSON object containing the declared outputs.", Value::Object(input_json)))]);
        if !matches!(stop_reason, ToolCallingStopReason::Done) { return Err(Error::ValidationError(format!("subagent '{}' did not complete normally: {stop_reason:?}", self.definition.id))); }
        let text = messages.last().and_then(LLMMessage::content_text_owned).ok_or_else(|| Error::ValidationError(format!("subagent '{}' returned no text", self.definition.id)))?;
        let output: Map<String, Value> = serde_json::from_str(text.trim()).map_err(|error| Error::ValidationError(format!("subagent '{}' returned invalid output JSON: {error}", self.definition.id)))?;
        let mut values = HashMap::new();
        for port in &self.definition.outputs {
            let value = output.get(&port.name);
            if port.required && value.is_none() { return Err(Error::ValidationError(format!("subagent '{}' missing required output '{}'", self.definition.id, port.name))); }
            if let Some(value) = value { values.insert(port.name.clone(), data_value_from_json_with_declared_type(port, value)?); }
        }
        Ok(values)
    }
}

pub struct SubAgentTool { agent: Arc<SubAgent> }
impl SubAgentTool { pub fn new(agent: Arc<SubAgent>) -> Self { Self { agent } } }
impl Tool for SubAgentTool {
    fn spec(&self) -> Arc<dyn FunctionTool> { Arc::new(SubAgentToolSpec { definition: self.agent.definition.clone() }) }
    fn execute(&self, _content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let input = subagent_input_from_tool_arguments(&self.agent.definition.inputs, arguments)?;
            let output = crate::runtime::block_async(self.agent.run(AgentContext::default(), input))?;
            Ok(Value::Object(output.into_iter().map(|(key, value)| (key, value.to_json())).collect()))
        })();
        result.unwrap_or_else(|error| json!({"ok": false, "error": error.to_string()})).to_string()
    }
}

fn subagent_input_from_tool_arguments(inputs: &[FunctionPortDef], arguments: &Value) -> Result<HashMap<String, DataValue>> {
    let object = arguments.as_object().ok_or_else(|| Error::ValidationError("subagent arguments must be an object".to_string()))?;
    let mut input = HashMap::new();
    for port in inputs {
        if let Some(value) = object.get(&port.name) {
            input.insert(port.name.clone(), data_value_from_json_with_declared_type(port, value)?);
        } else if port.required {
            return Err(Error::ValidationError(format!("missing required subagent tool argument '{}'", port.name)));
        }
    }
    Ok(input)
}

#[derive(Debug)]
struct SubAgentToolSpec { definition: SubAgentDefinition }
impl FunctionTool for SubAgentToolSpec {
    fn name(&self) -> &str { &self.definition.id }
    fn description(&self) -> &str { &self.definition.name }
    fn parameters(&self) -> Value { json!({"type":"object", "properties": self.definition.inputs.iter().map(|port| (port.name.clone(), json!({"description":port.description}))).collect::<Map<_, _>>(), "required": self.definition.inputs.iter().filter(|port| port.required).map(|port| port.name.clone()).collect::<Vec<_>>(), "additionalProperties":false}) }
    fn call(&self, arguments: Value) -> Result<Value> { Ok(arguments) }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use super::*;
    use crate::graph::DataType;

    fn available_tools() -> HashSet<String> {
        ["search_memory", "update_memory", "list_memory_keys"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    }

    #[test]
    fn definition_round_trips_through_yaml() {
        let definition = SubAgentDefinition { id: "memory".to_string(), name: "Memory".to_string(), inputs: vec![], outputs: vec![], system_prompt: String::new(), tool_ids: vec!["search_memory".to_string()] };
        let yaml = serde_yaml::to_string(&definition).unwrap();
        let parsed: SubAgentDefinition = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, definition);
        parsed.validate(&available_tools()).unwrap();
    }

    #[test]
    fn validation_rejects_duplicate_ports_and_unauthorized_tools() {
        let mut definition = SubAgentDefinition { id: "memory".to_string(), name: "Memory".to_string(), inputs: vec![FunctionPortDef { name: "content".to_string(), data_type: DataType::String, description: String::new(), required: true }], outputs: vec![], system_prompt: String::new(), tool_ids: vec!["search_memory".to_string()] };
        definition.inputs.push(FunctionPortDef { name: "content".to_string(), data_type: DataType::String, description: String::new(), required: true });
        assert!(definition.validate(&available_tools()).is_err());

        let mut definition = SubAgentDefinition { id: "memory".to_string(), name: "Memory".to_string(), inputs: vec![], outputs: vec![], system_prompt: String::new(), tool_ids: vec!["search_memory".to_string()] };
        definition.tool_ids.push("not_allowed".to_string());
        assert!(definition.validate(&available_tools()).is_err());
    }

    #[test]
    fn validation_rejects_invalid_and_conflicting_ids() {
        assert!(validate_subagent_id("Research").is_err());
        assert!(validate_subagent_id("../research").is_err());
        let definition = SubAgentDefinition { id: "search_memory".to_string(), name: "Search".to_string(), inputs: vec![], outputs: vec![], system_prompt: String::new(), tool_ids: vec![] };
        assert!(definition.validate(&available_tools()).is_err());
    }

    #[test]
    fn declared_ports_convert_json_arguments() {
        let port = FunctionPortDef { name: "count".to_string(), data_type: DataType::Integer, description: String::new(), required: true };
        let value = data_value_from_json_with_declared_type(&port, &json!(7)).unwrap();
        assert!(matches!(value, DataValue::Integer(7)));
        assert!(data_value_from_json_with_declared_type(&port, &json!("seven")).is_err());
    }

    #[test]
    fn tool_arguments_follow_declared_input_ports() {
        let inputs = vec![FunctionPortDef { name: "content".to_string(), data_type: DataType::String, description: String::new(), required: true }];
        let input = subagent_input_from_tool_arguments(&inputs, &json!({ "content": "hello" })).unwrap();
        assert!(matches!(input.get("content"), Some(DataValue::String(value)) if value == "hello"));
        assert!(subagent_input_from_tool_arguments(&inputs, &json!({})).is_err());
    }
}
