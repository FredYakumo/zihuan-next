use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::SharedTool;
use crate::agent::tools::{Tool, ToolCallingEngine, ToolCallingStopReason};
use crate::error::{Error, Result};
use crate::graph::function_graph::FunctionPortDef;
use crate::graph::util::function::data_value_from_json_with_declared_type;
use crate::graph::DataValue;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::tooling::FunctionTool;
use crate::model_inference::llm::{LLMMessage, MessageRole};
use crate::task_context::append_current_task_progress;
use crate::tool_runtime::ToolRunDuration;
use crate::tool_subgraph::data_type_to_json_schema_type;

/// Built-in agent definitions shipped with the application, embedded so a fresh install can seed
/// its `sub_agents` directory without any Rust-side definition.
///
/// Paths are anchored at the crate root so they survive moving this file within `zihuan_core`.
const BUILTIN_AGENT_DEFINITIONS: &[(&str, &str)] = &[
    (
        "memory_agent",
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../sub_agents/memory_agent.yaml")),
    ),
    (
        "run_research_subagent",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../sub_agents/run_research_subagent.yaml"
        )),
    ),
];

/// Agent definitions live in the `sub_agents` directory relative to the current working
/// directory, one YAML file per id.
pub fn agent_dir() -> PathBuf {
    PathBuf::from("sub_agents")
}

/// Creates the built-in definitions that are absent at application startup.
/// Existing files are intentionally left untouched.
pub fn seed_builtin_agents() -> Result<()> {
    seed_builtin_agents_at(&agent_dir())
}

fn seed_builtin_agents_at(directory: &Path) -> Result<()> {
    fs::create_dir_all(directory).map_err(|error| {
        Error::ValidationError(format!("failed to create agent definition directory: {error}"))
    })?;

    for (id, content) in BUILTIN_AGENT_DEFINITIONS {
        let path = directory.join(format!("{id}.yaml"));
        if path.exists() {
            continue;
        }
        fs::write(&path, content).map_err(|error| {
            Error::ValidationError(format!(
                "failed to write default agent '{}': {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

/// How a YAML agent turns its final assistant message into a tool result.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum YamlAgentOutputMode {
    /// Parse the final assistant message as JSON and map it onto the declared output ports.
    #[default]
    JsonPorts,
    /// Return the raw final assistant text; declared output ports are informational only.
    Text,
}

/// A conditional fragment appended to the rendered user prompt.
///
/// `port` names an input port. When `equals` is set the fragment is appended only if that
/// input's value equals it; otherwise the fragment is appended whenever the input is present
/// and non-empty. `{placeholders}` inside `template` are substituted from the input values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct YamlAgentPromptPart {
    pub port: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<String>,
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct YamlAgentDefinition {
    pub id: String,
    pub name: String,
    /// LLM-facing tool description; falls back to `name` when empty.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub inputs: Vec<FunctionPortDef>,
    #[serde(default)]
    pub outputs: Vec<FunctionPortDef>,
    pub system_prompt: String,
    /// User-message template. `None` keeps the default JSON `Input:` envelope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompt_parts: Vec<YamlAgentPromptPart>,
    #[serde(default)]
    pub output_mode: YamlAgentOutputMode,
    /// LLM handle kind the host resolves for this agent; falls back to `main`.
    #[serde(default = "default_llm_kind")]
    pub llm_kind: String,
    /// Dashboard task progress emitted before the run, rendered from input ports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_message: Option<String>,
    /// Merge the host's registered node-graph tools into this agent's tool set.
    #[serde(default)]
    pub include_graph_tools: bool,
    #[serde(default)]
    pub run_duration: ToolRunDuration,
    #[serde(default)]
    pub tool_ids: Vec<String>,
}

fn default_llm_kind() -> String {
    "main".to_string()
}

impl YamlAgentDefinition {
    pub fn validate(&self, available_tool_ids: &HashSet<String>) -> Result<()> {
        validate_agent_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err(Error::ValidationError("agent name must not be empty".to_string()));
        }
        validate_ports("input", &self.inputs)?;
        validate_ports("output", &self.outputs)?;
        if self.output_mode == YamlAgentOutputMode::Text {
            if self.outputs.len() > 1 {
                return Err(Error::ValidationError(format!(
                    "agent '{}' uses text output but declares more than one output port",
                    self.id
                )));
            }
            if let Some(port) = self.outputs.first() {
                if port.data_type != crate::graph::DataType::String {
                    return Err(Error::ValidationError(format!(
                        "agent '{}' uses text output but output port '{}' is not a String",
                        self.id, port.name
                    )));
                }
            }
        }
        let mut seen = HashSet::new();
        for tool_id in &self.tool_ids {
            if tool_id.trim().is_empty() || !seen.insert(tool_id) {
                return Err(Error::ValidationError(format!(
                    "agent '{}' has duplicate or empty tool id",
                    self.id
                )));
            }
            if tool_id == &self.id {
                return Err(Error::ValidationError(format!(
                    "agent '{}' cannot reference itself as a tool",
                    self.id
                )));
            }
            if !available_tool_ids.contains(tool_id) {
                return Err(Error::ValidationError(format!(
                    "agent '{}' is not allowed to use tool '{tool_id}'",
                    self.id
                )));
            }
        }
        for part in &self.prompt_parts {
            if !self.inputs.iter().any(|port| port.name == part.port) {
                return Err(Error::ValidationError(format!(
                    "agent '{}' prompt part references unknown input '{}'",
                    self.id, part.port
                )));
            }
        }
        Ok(())
    }
}

pub fn validate_agent_id(id: &str) -> Result<()> {
    let mut characters = id.chars();
    let Some(first) = characters.next() else {
        return Err(Error::ValidationError("agent id must not be empty".to_string()));
    };
    if !first.is_ascii_lowercase()
        || !characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(Error::ValidationError("agent id must start with a lowercase letter and contain only lowercase letters, numbers, or underscores".to_string()));
    }
    Ok(())
}

fn validate_ports(kind: &str, ports: &[FunctionPortDef]) -> Result<()> {
    let mut names = HashSet::new();
    for port in ports {
        let name = port.name.trim();
        if name.is_empty() || !names.insert(name) {
            return Err(Error::ValidationError(format!(
                "agent {kind} ports must have unique non-empty names"
            )));
        }
    }
    Ok(())
}

pub fn load_agent_definition(
    id: &str,
    available_tool_ids: &HashSet<String>,
) -> Result<YamlAgentDefinition> {
    validate_agent_id(id)?;
    let path = agent_dir().join(format!("{id}.yaml"));
    let content = fs::read_to_string(&path).map_err(|error| {
        Error::ValidationError(format!("failed to read agent '{}': {error}", path.display()))
    })?;
    let definition: YamlAgentDefinition = serde_yaml::from_str(&content).map_err(|error| {
        Error::ValidationError(format!("invalid agent '{}': {error}", path.display()))
    })?;
    definition.validate(available_tool_ids)?;
    Ok(definition)
}

pub fn save_agent_definition(
    definition: &YamlAgentDefinition,
    available_tool_ids: &HashSet<String>,
) -> Result<()> {
    definition.validate(available_tool_ids)?;
    let directory = agent_dir();
    fs::create_dir_all(&directory).map_err(|error| {
        Error::ValidationError(format!("failed to create agent definition directory: {error}"))
    })?;
    save_agent_definition_at(&directory.join(format!("{}.yaml", definition.id)), definition)
}

pub fn list_agent_definitions(
    available_tool_ids: &HashSet<String>,
) -> Result<Vec<YamlAgentDefinition>> {
    let directory = agent_dir();
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut definitions = fs::read_dir(&directory)
        .map_err(|error| {
            Error::ValidationError(format!("failed to read agent definition directory: {error}"))
        })?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("yaml"))
        .map(|entry| {
            let path = entry.path();
            let content = fs::read_to_string(&path).map_err(|error| {
                Error::ValidationError(format!(
                    "failed to read agent '{}': {error}",
                    path.display()
                ))
            })?;
            let definition: YamlAgentDefinition =
                serde_yaml::from_str(&content).map_err(|error| {
                    Error::ValidationError(format!("invalid agent '{}': {error}", path.display()))
                })?;
            definition.validate(available_tool_ids)?;
            Ok(definition)
        })
        .collect::<Result<Vec<_>>>()?;
    definitions.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(definitions)
}

/// Lists the ids of the agent definitions present on disk, without validating them.
///
/// Used to build the allow-list for `tool_ids` so one agent can reference another as a tool.
pub fn list_agent_ids() -> Vec<String> {
    let directory = agent_dir();
    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut ids = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|value| value.to_str()) == Some("yaml"))
                .then(|| path.file_stem().and_then(|value| value.to_str()).map(ToOwned::to_owned))
                .flatten()
        })
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

pub fn delete_agent_definition(id: &str) -> Result<()> {
    validate_agent_id(id)?;
    let path = agent_dir().join(format!("{id}.yaml"));
    if !path.exists() {
        return Err(Error::ValidationError(format!("agent '{id}' not found")));
    }
    fs::remove_file(&path).map_err(|error| {
        Error::ValidationError(format!("failed to delete agent '{}': {error}", path.display()))
    })
}

fn save_agent_definition_at(path: &Path, definition: &YamlAgentDefinition) -> Result<()> {
    let yaml = serde_yaml::to_string(definition).map_err(|error| {
        Error::ValidationError(format!("failed to serialize agent '{}': {error}", definition.id))
    })?;
    fs::write(path, yaml).map_err(|error| {
        Error::ValidationError(format!("failed to write agent '{}': {error}", path.display()))
    })
}

/// Result of one agent turn, shaped by [`YamlAgentOutputMode`].
#[derive(Debug, Clone)]
pub enum YamlAgentOutput {
    Text(String),
    Ports(HashMap<String, DataValue>),
}

impl YamlAgentOutput {
    /// Renders the output as the string handed back to the calling LLM.
    pub fn as_tool_result(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Ports(values) => Value::Object(
                values.iter().map(|(key, value)| (key.clone(), value.to_json())).collect(),
            )
            .to_string(),
        }
    }
}

pub struct YamlAgent {
    definition: YamlAgentDefinition,
    llm: Arc<dyn LLMBase>,
    tools: HashMap<String, Arc<dyn Tool>>,
    graph_tools: Vec<Arc<dyn Tool>>,
}

impl YamlAgent {
    pub fn new(
        definition: YamlAgentDefinition,
        llm: Arc<dyn LLMBase>,
        tools: HashMap<String, Arc<dyn Tool>>,
        graph_tools: Vec<Arc<dyn Tool>>,
    ) -> Result<Self> {
        definition.validate(&tools.keys().cloned().collect())?;
        Ok(Self { definition, llm, tools, graph_tools })
    }

    pub fn definition(&self) -> &YamlAgentDefinition {
        &self.definition
    }

    /// Builds the messages for one turn: the system prompt plus the rendered user prompt.
    fn build_messages(&self, input: &HashMap<String, DataValue>) -> Vec<LLMMessage> {
        vec![
            LLMMessage::system(self.definition.system_prompt.clone()),
            LLMMessage::user(render_user_prompt(&self.definition, input)),
        ]
    }
}

/// Renders the user message: the base template (or the default JSON envelope) followed by any
/// conditional prompt parts whose input condition matches.
fn render_user_prompt(
    definition: &YamlAgentDefinition,
    input: &HashMap<String, DataValue>,
) -> String {
    let mut user = match definition.user_prompt.as_deref() {
        Some(template) => render_template(template, input),
        None => {
            let input_json = input
                .iter()
                .map(|(key, value)| (key.clone(), value.to_json()))
                .collect::<Map<_, _>>();
            format!(
                "Input:\n{}\n\nReturn only a JSON object containing the declared outputs.",
                Value::Object(input_json)
            )
        }
    };
    for part in &definition.prompt_parts {
        let Some(value) = input.get(&part.port) else {
            continue;
        };
        let rendered = data_value_to_display_string(value);
        match part.equals.as_deref() {
            Some(expected) if expected != rendered => continue,
            None if rendered.trim().is_empty() => continue,
            _ => {}
        }
        user.push_str(&render_template(&part.template, input));
    }
    user
}

impl YamlAgent {
    /// Validates the declared input ports, runs one tool-calling turn, and shapes the result
    /// according to the definition's output mode.
    pub fn run(&self, input: HashMap<String, DataValue>) -> Result<YamlAgentOutput> {
        for port in &self.definition.inputs {
            let value = input.get(&port.name);
            if port.required && value.is_none() {
                return Err(Error::ValidationError(format!(
                    "agent '{}' missing required input '{}'",
                    self.definition.id, port.name
                )));
            }
            if let Some(value) =
                value.filter(|value| !port.data_type.is_compatible_with(&value.data_type()))
            {
                return Err(Error::ValidationError(format!(
                    "agent '{}' input '{}' expected {}, got {}",
                    self.definition.id,
                    port.name,
                    port.data_type,
                    value.data_type()
                )));
            }
        }

        if let Some(progress) = self.definition.progress_message.as_deref() {
            append_current_task_progress(render_template(progress, &input));
        }

        let mut engine = ToolCallingEngine::new(Arc::clone(&self.llm));
        for tool_id in &self.definition.tool_ids {
            engine.add_tool(SharedTool::new(Arc::clone(
                self.tools.get(tool_id).expect("validated tool id"),
            )));
        }
        if self.definition.include_graph_tools {
            for tool in &self.graph_tools {
                engine.add_tool(SharedTool::new(Arc::clone(tool)));
            }
        }

        let (messages, stop_reason) = engine.run(self.build_messages(&input));
        if !matches!(stop_reason, ToolCallingStopReason::Done) {
            return Err(Error::ValidationError(format!(
                "agent '{}' did not complete normally: {stop_reason:?}",
                self.definition.id
            )));
        }
        let text = messages
            .iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
            .and_then(LLMMessage::content_text_owned)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .ok_or_else(|| {
                Error::ValidationError(format!("agent '{}' returned no text", self.definition.id))
            })?;

        match self.definition.output_mode {
            YamlAgentOutputMode::Text => Ok(YamlAgentOutput::Text(text)),
            YamlAgentOutputMode::JsonPorts => {
                let output: Map<String, Value> = serde_json::from_str(&text).map_err(|error| {
                    Error::ValidationError(format!(
                        "agent '{}' returned invalid output JSON: {error}",
                        self.definition.id
                    ))
                })?;
                let mut values = HashMap::new();
                for port in &self.definition.outputs {
                    let value = output.get(&port.name);
                    if port.required && value.is_none() {
                        return Err(Error::ValidationError(format!(
                            "agent '{}' missing required output '{}'",
                            self.definition.id, port.name
                        )));
                    }
                    if let Some(value) = value {
                        values.insert(
                            port.name.clone(),
                            data_value_from_json_with_declared_type(port, value)?,
                        );
                    }
                }
                Ok(YamlAgentOutput::Ports(values))
            }
        }
    }

    /// Runs one turn synchronously and requires [`YamlAgentOutputMode::Text`].
    pub fn run_text(&self, input: HashMap<String, DataValue>) -> Result<String> {
        match self.run(input)? {
            YamlAgentOutput::Text(text) => Ok(text),
            YamlAgentOutput::Ports(_) => Err(Error::ValidationError(format!(
                "agent '{}' is not configured for text output",
                self.definition.id
            ))),
        }
    }
}

fn data_value_to_display_string(value: &DataValue) -> String {
    match value {
        DataValue::String(text) => text.clone(),
        other => other
            .to_json()
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| other.to_json().to_string()),
    }
}

/// Substitutes `{port_name}` placeholders with the matching input value; unknown or malformed
/// placeholders are left empty so prompts never leak raw template syntax.
fn render_template(template: &str, input: &HashMap<String, DataValue>) -> String {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('}') {
            let name = &after[..end];
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                if let Some(value) = input.get(name) {
                    output.push_str(&data_value_to_display_string(value));
                }
                rest = &after[end + 1..];
                continue;
            }
        }
        output.push('{');
        rest = after;
    }
    output.push_str(rest);
    output
}

/// Registry that resolves an agent's `tool_ids` and `llm_kind` against constructed instances,
/// and builds runnable [`YamlAgent`]s (`YamlAgentTool`s) from YAML definitions.
#[derive(Default)]
pub struct YamlAgentHost {
    tools: HashMap<String, Arc<dyn Tool>>,
    graph_tools: Vec<Arc<dyn Tool>>,
    llms: HashMap<String, Arc<dyn LLMBase>>,
}

impl YamlAgentHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a constructed tool under the id referenced by `tool_ids`.
    pub fn register_tool(&mut self, id: impl Into<String>, tool: Arc<dyn Tool>) -> &mut Self {
        self.tools.insert(id.into(), tool);
        self
    }

    /// Registers a node-graph tool; included by agents with `include_graph_tools: true`.
    pub fn register_graph_tool(&mut self, tool: Arc<dyn Tool>) -> &mut Self {
        self.graph_tools.push(tool);
        self
    }

    /// Registers the LLM handle a `llm_kind` resolves to; `main` is the fallback.
    pub fn register_llm(&mut self, kind: impl Into<String>, llm: Arc<dyn LLMBase>) -> &mut Self {
        self.llms.insert(kind.into(), llm);
        self
    }

    /// Registers a placeholder for an id whose backing resource is unavailable, keeping
    /// definitions that reference it resolvable while making the failure explicit on call.
    pub fn register_disabled_tool(
        &mut self,
        id: impl Into<String>,
        reason: impl Into<String>,
    ) -> &mut Self {
        let id = id.into();
        self.tools.insert(id.clone(), Arc::new(DisabledTool::new(id, reason)));
        self
    }

    pub fn register_disabled_tools(
        &mut self,
        ids: impl IntoIterator<Item = impl Into<String>>,
        reason: &str,
    ) -> &mut Self {
        for id in ids {
            self.register_disabled_tool(id, reason.to_string());
        }
        self
    }

    pub fn tool(&self, id: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(id).cloned()
    }

    pub fn available_tool_ids(&self) -> HashSet<String> {
        self.tools.keys().cloned().collect()
    }

    pub fn load_definition(&self, id: &str) -> Result<YamlAgentDefinition> {
        load_agent_definition(id, &self.available_tool_ids())
    }

    /// Builds an agent from an already-parsed definition without registering it as a tool.
    pub fn build_definition(&self, definition: YamlAgentDefinition) -> Result<YamlAgent> {
        let llm = self
            .llms
            .get(definition.llm_kind.as_str())
            .or_else(|| self.llms.get("main"))
            .cloned()
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "agent '{}' has no registered LLM for kind '{}'",
                    definition.id, definition.llm_kind
                ))
            })?;
        let mut tools = HashMap::new();
        for tool_id in &definition.tool_ids {
            let tool = self.tools.get(tool_id).cloned().ok_or_else(|| {
                Error::ValidationError(format!(
                    "agent '{}' references unregistered tool '{tool_id}'",
                    definition.id
                ))
            })?;
            tools.insert(tool_id.clone(), tool);
        }
        let graph_tools = if definition.include_graph_tools {
            self.graph_tools.clone()
        } else {
            Vec::new()
        };
        YamlAgent::new(definition, llm, tools, graph_tools)
    }

    /// Builds an agent from its YAML definition without registering it as a tool.
    pub fn build(&self, id: &str) -> Result<YamlAgent> {
        self.build_definition(self.load_definition(id)?)
    }

    /// Builds an agent and registers it as a callable tool under its id.
    pub fn publish(&mut self, id: &str) -> Result<Arc<dyn Tool>> {
        let agent = Arc::new(self.build(id)?);
        let tool: Arc<dyn Tool> = Arc::new(YamlAgentTool::new(Arc::clone(&agent)));
        self.tools.insert(id.to_string(), Arc::clone(&tool));
        Ok(tool)
    }

    /// [`Self::publish`] that logs the reason and returns `None` instead of propagating, for
    /// call sites whose surrounding builder cannot report errors.
    pub fn publish_logged(&mut self, id: &str) -> Option<Arc<dyn Tool>> {
        match self.publish(id) {
            Ok(tool) => Some(tool),
            Err(error) => {
                log::warn!("agent '{}' could not be published: {error}", id);
                None
            }
        }
    }
}

pub struct YamlAgentTool {
    agent: Arc<YamlAgent>,
}

/// Stand-in for a tool id whose backing resource is unavailable; every call returns `reason`.
pub struct DisabledTool {
    name: String,
    description: String,
    reason: String,
}

impl DisabledTool {
    pub fn new(name: impl Into<String>, reason: impl Into<String>) -> Self {
        let name = name.into();
        let description = format!("{name} (unavailable)");
        Self { name, description, reason: reason.into() }
    }
}

impl Tool for DisabledTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(DisabledToolSpec {
            name: self.name.clone(),
            description: self.description.clone(),
        })
    }
    fn execute(&self, _content: &str, _arguments: &Value) -> String {
        json!({"ok": false, "error": self.reason}).to_string()
    }
}

#[derive(Debug)]
struct DisabledToolSpec {
    name: String,
    description: String,
}

impl FunctionTool for DisabledToolSpec {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn parameters(&self) -> Value {
        json!({"type": "object"})
    }
    fn call(&self, arguments: Value) -> Result<Value> {
        Ok(arguments)
    }
}
impl YamlAgentTool {
    pub fn new(agent: Arc<YamlAgent>) -> Self {
        Self { agent }
    }
}
impl Tool for YamlAgentTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(YamlAgentToolSpec {
            definition: self.agent.definition.clone(),
        })
    }
    fn execute(&self, _content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let input = agent_input_from_tool_arguments(&self.agent.definition.inputs, arguments)?;
            let output = self.agent.run(input)?;
            Ok(output.as_tool_result())
        })();
        result.unwrap_or_else(|error| json!({"ok": false, "error": error.to_string()}).to_string())
    }
    fn run_duration(&self) -> ToolRunDuration {
        self.agent.definition.run_duration
    }
}

fn agent_input_from_tool_arguments(
    inputs: &[FunctionPortDef],
    arguments: &Value,
) -> Result<HashMap<String, DataValue>> {
    let object = arguments
        .as_object()
        .ok_or_else(|| Error::ValidationError("agent arguments must be an object".to_string()))?;
    let mut input = HashMap::new();
    for port in inputs {
        if let Some(value) = object.get(&port.name) {
            input.insert(port.name.clone(), data_value_from_json_with_declared_type(port, value)?);
        } else if port.required {
            return Err(Error::ValidationError(format!(
                "missing required agent tool argument '{}'",
                port.name
            )));
        }
    }
    Ok(input)
}

#[derive(Debug)]
struct YamlAgentToolSpec {
    definition: YamlAgentDefinition,
}
impl FunctionTool for YamlAgentToolSpec {
    fn name(&self) -> &str {
        &self.definition.id
    }
    fn description(&self) -> &str {
        if self.definition.description.trim().is_empty() {
            &self.definition.name
        } else {
            &self.definition.description
        }
    }
    fn parameters(&self) -> Value {
        let properties = self
            .definition
            .inputs
            .iter()
            .map(|port| {
                (
                    port.name.clone(),
                    json!({
                        "type": data_type_to_json_schema_type(&port.data_type),
                        "description": port.description,
                    }),
                )
            })
            .collect::<Map<_, _>>();
        let required = self
            .definition
            .inputs
            .iter()
            .filter(|port| port.required)
            .map(|port| port.name.clone())
            .collect::<Vec<_>>();
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        })
    }
    fn call(&self, arguments: Value) -> Result<Value> {
        Ok(arguments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::DataType;

    fn available_tools() -> HashSet<String> {
        ["search_memory", "update_memory", "list_memory_keys"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    }

    fn sample_definition(id: &str) -> YamlAgentDefinition {
        YamlAgentDefinition {
            id: id.to_string(),
            name: "Memory".to_string(),
            description: String::new(),
            builtin: false,
            inputs: vec![],
            outputs: vec![],
            system_prompt: String::new(),
            user_prompt: None,
            prompt_parts: vec![],
            output_mode: YamlAgentOutputMode::JsonPorts,
            llm_kind: default_llm_kind(),
            progress_message: None,
            include_graph_tools: false,
            run_duration: ToolRunDuration::Short,
            tool_ids: vec!["search_memory".to_string()],
        }
    }

    #[test]
    fn definition_round_trips_through_yaml() {
        let definition = sample_definition("memory");
        let yaml = serde_yaml::to_string(&definition).unwrap();
        let parsed: YamlAgentDefinition = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, definition);
        parsed.validate(&available_tools()).unwrap();
    }

    #[test]
    fn legacy_definition_without_new_fields_parses() {
        let yaml = "id: memory\nname: Memory\nsystem_prompt: hi\ntool_ids: [search_memory]\n";
        let parsed: YamlAgentDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.output_mode, YamlAgentOutputMode::JsonPorts);
        assert!(parsed.user_prompt.is_none());
        parsed.validate(&available_tools()).unwrap();
    }

    #[test]
    fn validation_rejects_duplicate_ports_and_unauthorized_tools() {
        let mut definition = sample_definition("memory");
        definition.inputs = vec![FunctionPortDef {
            name: "content".to_string(),
            data_type: DataType::String,
            description: String::new(),
            required: true,
        }];
        definition.inputs.push(FunctionPortDef {
            name: "content".to_string(),
            data_type: DataType::String,
            description: String::new(),
            required: true,
        });
        assert!(definition.validate(&available_tools()).is_err());

        let mut definition = sample_definition("memory");
        definition.tool_ids.push("not_allowed".to_string());
        assert!(definition.validate(&available_tools()).is_err());
    }

    #[test]
    fn validation_rejects_invalid_and_conflicting_ids() {
        assert!(validate_agent_id("Research").is_err());
        assert!(validate_agent_id("../research").is_err());
        let definition = sample_definition("search_memory");
        assert!(definition.validate(&available_tools()).is_err());
    }

    #[test]
    fn declared_ports_convert_json_arguments() {
        let port = FunctionPortDef {
            name: "count".to_string(),
            data_type: DataType::Integer,
            description: String::new(),
            required: true,
        };
        let value = data_value_from_json_with_declared_type(&port, &json!(7)).unwrap();
        assert!(matches!(value, DataValue::Integer(7)));
        assert!(data_value_from_json_with_declared_type(&port, &json!("seven")).is_err());
    }

    #[test]
    fn tool_arguments_follow_declared_input_ports() {
        let inputs = vec![FunctionPortDef {
            name: "content".to_string(),
            data_type: DataType::String,
            description: String::new(),
            required: true,
        }];
        let input =
            agent_input_from_tool_arguments(&inputs, &json!({ "content": "hello" })).unwrap();
        assert!(matches!(input.get("content"), Some(DataValue::String(value)) if value == "hello"));
        assert!(agent_input_from_tool_arguments(&inputs, &json!({})).is_err());
    }

    #[test]
    fn template_renders_known_ports_and_drops_unknown() {
        let input = HashMap::from([
            ("problem".to_string(), DataValue::String("ping".to_string())),
            ("count".to_string(), DataValue::Integer(3)),
        ]);
        assert_eq!(render_template("{problem}/{count}/{missing}", &input), "ping/3/");
    }

    #[test]
    fn prompt_parts_match_on_input_value() {
        let definition = YamlAgentDefinition {
            user_prompt: Some("{chat_context}".to_string()),
            prompt_parts: vec![YamlAgentPromptPart {
                port: "operation".to_string(),
                equals: Some("search_memory".to_string()),
                template: "\nSEARCH".to_string(),
            }],
            tool_ids: vec![],
            ..sample_definition("memory")
        };
        let input = HashMap::from([(
            "operation".to_string(),
            DataValue::String("search_memory".to_string()),
        )]);
        assert!(render_user_prompt(&definition, &input).ends_with("\nSEARCH"));

        let other = HashMap::from([(
            "operation".to_string(),
            DataValue::String("update_memory".to_string()),
        )]);
        assert!(!render_user_prompt(&definition, &other).contains("SEARCH"));
    }
}
