use std::collections::HashMap;
use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;

use tokio::task::block_in_place;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::graph::data_value::{LLMMessageSessionCacheRef, SessionClaim, SessionStateRef, SESSION_CLAIM_CONTEXT};
use dynamic_script_engine::{NodeRuntimeConfig, ScriptLanguage};

use super::registry::{NodeFactory, NodeRegistry};
use super::{DataValue, Node, NodeConfigField, NodeConfigFlow, NodeInputFlow, NodeOutputFlow, Port, RuntimeVariableStore};

thread_local! {
    static SCRIPT_RESOURCES: RefCell<ScriptResourceStore> = RefCell::new(ScriptResourceStore::default());
}


#[derive(Default)]
struct ScriptResourceStore {
    next_id: u64,
    values: HashMap<String, DataValue>,
}

pub fn with_dynamic_script_resources<T>(operation: impl FnOnce() -> T) -> T {
    SCRIPT_RESOURCES.with(|store| *store.borrow_mut() = ScriptResourceStore::default());
    let result = operation();
    SCRIPT_RESOURCES.with(|store| *store.borrow_mut() = ScriptResourceStore::default());
    result
}

/// Start the process-wide dynamic script runtime.
///
/// The worker owns a line-oriented stdin/stdout protocol, so requests are
/// serialized through a mutex while the process itself remains alive.
pub fn start_dynamic_script_runtime() -> Result<()> {
    let workspace_root = std::env::current_dir()
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    let config = crate::config::ConfigCenter::shared().load_root()?;
    let catalog = dynamic_script_engine::load_script_catalog(
        &workspace_root,
        &config.node_runtime,
        &config.python_runtime,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    for diagnostic in catalog.diagnostics {
        log::warn!("dynamic script {:?}: {}", diagnostic.language, diagnostic.message);
    }
    for language in catalog
        .nodes
        .iter()
        .filter_map(|node| node.get("language").cloned())
        .filter_map(|language| serde_json::from_value::<ScriptLanguage>(language).ok())
        .collect::<std::collections::HashSet<_>>()
    {
        dynamic_script_engine::start_script_runtime(
            &workspace_root,
            language,
            &config.node_runtime,
            &config.python_runtime,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    }
    Ok(())
}

fn request_dynamic_script_runtime(
    language: ScriptLanguage,
    request: &Value,
    host: &mut dyn FnMut(&str, &Value) -> Result<Value>,
) -> Result<Value> {
    let workspace_root = std::env::current_dir()
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    let config = crate::config::ConfigCenter::shared().load_root()?;
    dynamic_script_engine::request_script_runtime(&workspace_root, language, &config.node_runtime, &config.python_runtime, request, &mut |method, params| {
        host(method, params).map_err(|error| error.to_string())
    }).map_err(|error| Error::ValidationError(error.to_string()))
}

fn is_opaque_script_resource(value: &DataValue) -> bool {
    matches!(
        value,
        DataValue::BotAdapterRef(_)
            | DataValue::S3Ref(_)
            | DataValue::RedisRef(_)
            | DataValue::RdbRef(_)
            | DataValue::WeaviateRef(_)
            | DataValue::WebSearchEngineRef(_)
            | DataValue::SessionStateRef(_)
            | DataValue::LLMMessageSessionCacheRef(_)
            | DataValue::LLModel(_)
            | DataValue::EmbeddingModel(_)
            | DataValue::LoopControlRef(_)
    )
}

fn script_value_to_json(value: &DataValue) -> Value {
    if is_opaque_script_resource(value) {
        return SCRIPT_RESOURCES.with(|store| {
            let mut store = store.borrow_mut();
            store.next_id += 1;
            let handle = format!("resource-{}", store.next_id);
            let data_type = value.data_type().to_string();
            store.values.insert(handle.clone(), value.clone());
            json!({"$zihuan_handle": handle, "data_type": data_type})
        });
    }
    match value {
        DataValue::Vec(_, values) => Value::Array(values.iter().map(script_value_to_json).collect()),
        _ => value.to_json(),
    }
}

fn store_script_resource(value: DataValue) -> Value {
    script_value_to_json(&value)
}

fn resource_from_json(value: &Value, expected_type: &super::DataType) -> Option<DataValue> {
    let handle = value.get("$zihuan_handle")?.as_str()?;
    SCRIPT_RESOURCES.with(|store| {
        let value = store.borrow().values.get(handle)?.clone();
        value.data_type().is_compatible_with(expected_type).then_some(value)
    })
}

fn script_json_to_value(value: &Value, expected_type: &super::DataType) -> Option<DataValue> {
    resource_from_json(value, expected_type)
        .or_else(|| super::registry::json_to_data_value(value, expected_type))
}

pub fn host_value_to_json(value: &DataValue) -> Value {
    script_value_to_json(value)
}

pub fn host_json_to_value(value: &Value, expected_type: &super::DataType) -> Option<DataValue> {
    script_json_to_value(value, expected_type)
}

fn session_resource(params: &Value) -> Result<Arc<SessionStateRef>> {
    let value = params.get("session_ref")
        .ok_or_else(|| Error::ValidationError("session_ref is required".to_string()))?;
    match resource_from_json(value, &super::DataType::SessionStateRef) {
        Some(DataValue::SessionStateRef(session_ref)) => Ok(session_ref),
        _ => Err(Error::ValidationError("session_ref must be a SessionStateRef handle".to_string())),
    }
}

fn cache_resource(params: &Value) -> Result<Arc<LLMMessageSessionCacheRef>> {
    let value = params.get("cache_ref")
        .ok_or_else(|| Error::ValidationError("cache_ref is required".to_string()))?;
    match resource_from_json(value, &super::DataType::LLMMessageSessionCacheRef) {
        Some(DataValue::LLMMessageSessionCacheRef(cache_ref)) => Ok(cache_ref),
        _ => Err(Error::ValidationError("cache_ref must be an LLMMessageSessionCacheRef handle".to_string())),
    }
}

fn llm_model_resource(params: &Value, field: &str) -> Result<Arc<dyn crate::model_inference::llm::llm_base::LLMBase>> {
    let value = params.get(field)
        .ok_or_else(|| Error::ValidationError(format!("{field} is required")))?;
    match resource_from_json(value, &super::DataType::LLModel) {
        Some(DataValue::LLModel(model)) => Ok(model),
        _ => Err(Error::ValidationError(format!("{field} must be an LLModel handle"))),
    }
}

fn embedding_model_resource(params: &Value) -> Result<Arc<dyn crate::model_inference::llm::embedding_base::EmbeddingBase>> {
    let value = params.get("embedding_model")
        .ok_or_else(|| Error::ValidationError("embedding_model is required".to_string()))?;
    match resource_from_json(value, &super::DataType::EmbeddingModel) {
        Some(DataValue::EmbeddingModel(model)) => Ok(model),
        _ => Err(Error::ValidationError("embedding_model must be an EmbeddingModel handle".to_string())),
    }
}

fn weaviate_resource(params: &Value, field: &str) -> Result<Arc<crate::weaviate::WeaviateRef>> {
    let value = params
        .get(field)
        .ok_or_else(|| Error::ValidationError(format!("{field} is required")))?;
    match resource_from_json(value, &super::DataType::WeaviateRef) {
        Some(DataValue::WeaviateRef(reference)) => Ok(reference),
        _ => Err(Error::ValidationError(format!("{field} must be a WeaviateRef handle"))),
    }
}

fn qq_messages_param(params: &Value, field: &str) -> Result<Vec<crate::ims_bot_adapter::models::message::Message>> {
    serde_json::from_value(
        params
            .get(field)
            .cloned()
            .ok_or_else(|| Error::ValidationError(format!("{field} is required")))?,
    )
    .map_err(|error| Error::ValidationError(format!("{field} must be Vec<QQMessage>: {error}")))
}

fn web_search_resource(params: &Value) -> Result<Arc<dyn crate::rag::WebSearchEngine>> {
    let value = params.get("tavily_ref")
        .ok_or_else(|| Error::ValidationError("tavily_ref is required".to_string()))?;
    match resource_from_json(value, &super::DataType::WebSearchEngineRef) {
        Some(DataValue::WebSearchEngineRef(engine)) => Ok(engine),
        _ => Err(Error::ValidationError("tavily_ref must be a WebSearchEngineRef handle".to_string())),
    }
}

fn bot_adapter_resource(
    params: &Value,
    field: &str,
) -> Result<crate::ims_bot_adapter::runtime::adapter::SharedBotAdapter> {
    let value = params
        .get(field)
        .ok_or_else(|| Error::ValidationError(format!("{field} is required")))?;
    match resource_from_json(value, &super::DataType::BotAdapterRef) {
        Some(DataValue::BotAdapterRef(handle)) => {
            Ok(crate::ims_bot_adapter::runtime::adapter::shared_from_handle(&handle))
        }
        _ => Err(Error::ValidationError(format!(
            "{field} must be a BotAdapterRef handle"
        ))),
    }
}

fn optional_s3_resource(params: &Value, field: &str) -> Result<Option<Arc<crate::graph::object_storage::S3Ref>>> {
    let Some(value) = params.get(field).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    match resource_from_json(value, &super::DataType::S3Ref) {
        Some(DataValue::S3Ref(reference)) => Ok(Some(reference)),
        _ => Err(Error::ValidationError(format!("{field} must be an S3Ref handle"))),
    }
}

fn mysql_resource(params: &Value, field: &str) -> Result<Arc<crate::data_refs::MySqlConfig>> {
    let value = params.get(field)
        .ok_or_else(|| Error::ValidationError(format!("{field} is required")))?;
    match resource_from_json(value, &super::DataType::RdbRef) {
        Some(DataValue::RdbRef(crate::data_refs::RelationalDbConnection::MySql(reference))) => Ok(reference),
        Some(DataValue::RdbRef(_)) => Err(Error::ValidationError(format!("{field} must be a MySQL RdbRef handle"))),
        _ => Err(Error::ValidationError(format!("{field} must be an RdbRef handle"))),
    }
}

fn optional_string(params: &Value, field: &str) -> Option<String> {
    params.get(field).and_then(Value::as_str).map(str::trim)
        .filter(|value| !value.is_empty()).map(ToOwned::to_owned)
}

fn message_event(params: &Value) -> Result<crate::ims_bot_adapter::runtime::models::event_model::MessageEvent> {
    let value = params.get("message_event")
        .ok_or_else(|| Error::ValidationError("message_event is required".to_string()))?;
    match script_json_to_value(value, &super::DataType::MessageEvent) {
        Some(DataValue::MessageEvent(event)) => Ok(event),
        _ => Err(Error::ValidationError("message_event must be a valid MessageEvent".to_string())),
    }
}

fn positive_limit(params: &Value, default: Option<i64>) -> Result<u32> {
    let limit = params.get("limit").and_then(Value::as_i64).or(default)
        .ok_or_else(|| Error::ValidationError("limit is required".to_string()))?;
    if limit <= 0 {
        return Err(Error::ValidationError("limit must be greater than 0".to_string()));
    }
    Ok(limit as u32)
}

fn history_messages_json(rows: Vec<sqlx::mysql::MySqlRow>, limit: usize) -> Value {
    let messages = crate::graph::message_rdb_history_common::format_history_messages(
        crate::graph::message_rdb_history_common::aggregate_history_rows(
            rows.into_iter().map(crate::graph::message_rdb_history_common::message_history_chunk_row_from_row).collect(),
            limit,
        ),
    );
    json!({"messages": messages})
}

fn llm_messages_from_json(value: &Value, field: &str) -> Result<Vec<crate::model_inference::llm::LLMMessage>> {
    let parsed = script_json_to_value(value, &super::DataType::Vec(Box::new(super::DataType::LLMMessage)))
        .ok_or_else(|| Error::ValidationError(format!("{field} must be Vec<LLMMessage>")))?;
    match parsed {
        DataValue::Vec(_, items) => items.into_iter().map(|item| match item {
            DataValue::LLMMessage(message) => Ok(message),
            _ => Err(Error::ValidationError(format!("{field} must contain LLMMessage items"))),
        }).collect(),
        _ => Err(Error::ValidationError(format!("{field} must be Vec<LLMMessage>"))),
    }
}

fn required_string(params: &Value, name: &str) -> Result<String> {
    params.get(name).and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| Error::ValidationError(format!("{name} is required")))
}

#[derive(Debug, Clone, Deserialize)]
pub struct DynamicScriptNodeDefinition {
    pub language: ScriptLanguage,
    pub type_id: String,
    pub display_name: String,
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_ports: Vec<Port>,
    #[serde(default)]
    pub output_ports: Vec<Port>,
    #[serde(default)]
    pub dynamic_input_ports: bool,
    #[serde(default)]
    pub dynamic_output_ports: bool,
    #[serde(default)]
    pub config_fields: Vec<NodeConfigField>,
}

#[derive(Debug, Deserialize)]
struct DynamicScriptNodePorts {
    input_ports: Vec<Port>,
    output_ports: Vec<Port>,
}

pub fn load_script_catalog(workspace_root: &Path, config: &NodeRuntimeConfig) -> Result<Vec<DynamicScriptNodeDefinition>> {
    let python = crate::config::ConfigCenter::shared().load_root()?.python_runtime;
    let catalog = dynamic_script_engine::load_script_catalog(workspace_root, config, &python)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    for diagnostic in catalog.diagnostics {
        log::warn!("dynamic script {:?}: {}", diagnostic.language, diagnostic.message);
    }
    let definitions: Vec<DynamicScriptNodeDefinition> = serde_json::from_value(Value::Array(catalog.nodes))
        .map_err(|error| Error::ValidationError(format!("动态脚本运行时目录不是合法 JSON: {error}")))?;
    let mut ids = std::collections::HashSet::new();
    for definition in &definitions {
        if definition.type_id.trim().is_empty() || !ids.insert(definition.type_id.clone()) {
            return Err(Error::ValidationError(format!("动态脚本运行时目录包含无效或重复类型: {}", definition.type_id)));
        }
    }
    Ok(definitions)
}

pub fn register_script_catalog(registry: &NodeRegistry, workspace_root: &Path, config: &NodeRuntimeConfig) -> Result<()> {
    for definition in load_script_catalog(workspace_root, config)? {
        let factory_definition = definition.clone();
        let factory: NodeFactory = Arc::new(move |id, name| {
            Box::new(DynamicScriptNode::new(id, name, factory_definition.clone()))
        });
        registry.register(
            definition.type_id,
            definition.display_name,
            definition.category,
            definition.description,
            factory,
        )?;
    }
    Ok(())
}

fn resolve_script_ports(language: ScriptLanguage, type_id: &str, inline_values: &HashMap<String, Value>) -> Result<DynamicScriptNodePorts> {
    let workspace_root = std::env::current_dir()
        .map_err(|error| Error::ValidationError(format!("无法获取动态脚本运行时工作目录: {error}")))?;
    let runtime = crate::config::ConfigCenter::shared().load_root()?;
    let request = json!({ "type_id": type_id, "inline_values": inline_values });
    let response = dynamic_script_engine::resolve_script_ports(&workspace_root, language, &runtime.node_runtime, &runtime.python_runtime, &request)
        .map_err(|error| Error::ValidationError(format!("解析动态脚本节点 '{type_id}' 的动态端口失败: {error}")))?;
    serde_json::from_value(response)
        .map_err(|error| Error::ValidationError(format!("动态脚本节点 '{}' 的端口响应无效: {error}", type_id)))
}

struct DynamicScriptNode {
    id: String,
    name: String,
    definition: DynamicScriptNodeDefinition,
    inline_values: HashMap<String, Value>,
    input_ports: Vec<Port>,
    output_ports: Vec<Port>,
    runtime_variables: Option<RuntimeVariableStore>,
}

impl DynamicScriptNode {
    fn new(id: String, name: String, definition: DynamicScriptNodeDefinition) -> Self {
        Self {
            id,
            name,
            input_ports: definition.input_ports.clone(),
            output_ports: definition.output_ports.clone(),
            definition,
            inline_values: HashMap::new(),
            runtime_variables: None,
        }
    }
}

impl Node for DynamicScriptNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> { Some(&self.definition.description) }
    fn input_ports(&self) -> Vec<Port> { self.input_ports.clone() }
    fn output_ports(&self) -> Vec<Port> { self.output_ports.clone() }
    fn config_fields(&self) -> Vec<NodeConfigField> { self.definition.config_fields.clone() }
    fn has_dynamic_input_ports(&self) -> bool { self.definition.dynamic_input_ports }
    fn has_dynamic_output_ports(&self) -> bool { self.definition.dynamic_output_ports }

    fn apply_inline_config(&mut self, values: &NodeConfigFlow) -> Result<()> {
        self.inline_values = values.iter().map(|(name, value)| (name.clone(), value.to_json())).collect();
        if self.definition.dynamic_input_ports || self.definition.dynamic_output_ports {
            let ports = resolve_script_ports(self.definition.language, &self.definition.type_id, &self.inline_values)?;
            self.input_ports = ports.input_ports;
            self.output_ports = ports.output_ports;
        }
        Ok(())
    }

    fn set_runtime_variable_store(&mut self, store: RuntimeVariableStore) {
        self.runtime_variables = Some(store);
    }

    fn execute(&mut self, inputs: NodeInputFlow) -> Result<NodeOutputFlow> {
        self.validate_inputs(&inputs)?;
        let request = json!({
            "type_id": self.definition.type_id,
            "node_id": self.id,
            "node_name": self.name,
            "inline_values": self.inline_values,
            "inputs": inputs.iter().map(|(name, value)| (name.clone(), script_value_to_json(value))).collect::<serde_json::Map<_, _>>(),
        });
        let variables = self.runtime_variables.clone();
        let response = request_dynamic_script_runtime(self.definition.language, &request, &mut |method, params| {
                match method {
                    "variables.get" => {
                        let name = params.get("name").and_then(Value::as_str)
                            .ok_or_else(|| Error::ValidationError("variables.get 缺少 name".to_string()))?;
                        Ok(variables.as_ref().and_then(|store| store.read().ok().and_then(|values| values.get(name).map(script_value_to_json))).unwrap_or(Value::Null))
                    }
                    "variables.set" => {
                        let name = params.get("name").and_then(Value::as_str)
                            .ok_or_else(|| Error::ValidationError("variables.set 缺少 name".to_string()))?;
                        let value = params.get("value").cloned().unwrap_or(Value::Null);
                        let Some(store) = &variables else {
                            return Err(Error::ValidationError("当前图没有运行时变量存储".to_string()));
                        };
                        let parsed = super::registry::json_to_data_value(&value, &super::DataType::Any)
                            .ok_or_else(|| Error::ValidationError("variables.set value 无效".to_string()))?;
                        store.write().map_err(|_| Error::ValidationError("运行时变量存储不可用".to_string()))?.insert(name, parsed);
                        Ok(Value::Bool(true))
                    }
                    "task.progress" => {
                        let message = params.get("message").and_then(Value::as_str)
                            .filter(|message| !message.trim().is_empty())
                            .ok_or_else(|| Error::ValidationError("task.progress 缺少 message".to_string()))?;
                        let task_id = crate::task_context::current_task_id()
                            .filter(|task_id| !task_id.trim().is_empty())
                            .ok_or_else(|| Error::ValidationError("当前节点未关联任务".to_string()))?;
                        let runtime = crate::command::global_task_runtime()
                            .ok_or_else(|| Error::ValidationError("任务运行时未初始化".to_string()))?;
                        runtime.append_task_progress(&task_id, message.to_string());
                        Ok(Value::Bool(true))
                    }
                    "task.append" => {
                        let task_id = optional_string(params, "task_id").unwrap_or_default();
                        let message = optional_string(params, "message").unwrap_or_default();
                        let ok = if task_id.is_empty() || message.is_empty() {
                            false
                        } else if let Some(runtime) = crate::command::global_task_runtime() {
                            runtime.append_task_progress(&task_id, message);
                            true
                        } else {
                            false
                        };
                        Ok(Value::Bool(ok))
                    }
                    "agent.llm" => {
                        let config = crate::agent::runtime_context::current_qq_chat_agent_service_config()?;
                        let kind = crate::agent::normalize_llm_kind(params.get("llm_kind").and_then(Value::as_str))?;
                        let model = crate::model_inference::agent_config_support::build_llm_from_ref_id(
                            crate::agent::qq_chat::llm_ref_id_for_kind(&config, kind),
                        )?;
                        Ok(store_script_resource(DataValue::LLModel(model)))
                    }
                    "agent.embedding_model" => {
                        let config = crate::agent::runtime_context::current_qq_chat_agent_service_config()?;
                        let model = crate::model_inference::agent_config_support::build_embedding_from_ref_id(
                            config.embedding_model_ref_id.as_deref(),
                        )?;
                        Ok(store_script_resource(DataValue::EmbeddingModel(model)))
                    }
                    "agent.task" => {
                        let task_id = crate::task_context::current_task_id().unwrap_or_default();
                        Ok(json!({"task_id": task_id, "has_task": !task_id.trim().is_empty()}))
                    }
                    "agent.rdb" => {
                        let config = crate::agent::runtime_context::current_qq_chat_agent_service_config()?;
                        let connection_id = config.resolved_rdb_id().map(str::trim).filter(|value| !value.is_empty())
                            .ok_or_else(|| Error::ValidationError("rdb_connection_id is required".to_string()))?;
                        let reference = crate::runtime::block_async(
                            crate::storage::RuntimeStorageConnectionManager::shared().get_or_create_mysql_ref(connection_id),
                        )?;
                        Ok(store_script_resource(DataValue::RdbRef(crate::data_refs::RelationalDbConnection::MySql(reference))))
                    }
                    "agent.s3" => {
                        let config = crate::agent::runtime_context::current_qq_chat_agent_service_config()?;
                        let connection_id = config.rustfs_connection_id.as_deref().map(str::trim).filter(|value| !value.is_empty())
                            .ok_or_else(|| Error::ValidationError("rustfs_connection_id is required".to_string()))?;
                        let reference = crate::runtime::block_async(
                            crate::storage::RuntimeStorageConnectionManager::shared().get_or_create_s3_ref(connection_id),
                        )?;
                        Ok(store_script_resource(DataValue::S3Ref(reference)))
                    }
                    "agent.image_weaviate" => {
                        let config = crate::agent::runtime_context::current_qq_chat_agent_service_config()?;
                        let connection_id = config.weaviate_image_connection_id.as_deref().map(str::trim).filter(|value| !value.is_empty())
                            .ok_or_else(|| Error::ValidationError("weaviate_image_connection_id is required".to_string()))?;
                        let reference = crate::storage::resource_resolver::build_weaviate_ref(
                            Some(connection_id), &crate::storage::load_connections()?,
                            Some(crate::storage::WeaviateCollectionSchema::ImageSemantic),
                        )?.ok_or_else(|| Error::ValidationError("weaviate_image_connection_id is required".to_string()))?;
                        crate::storage::ensure_collection_schema(&reference, crate::storage::WeaviateCollectionSchema::ImageSemantic, false)?;
                        Ok(store_script_resource(DataValue::WeaviateRef(reference)))
                    }
                    "agent.web_search" => {
                        let config = crate::agent::runtime_context::current_qq_chat_agent_service_config()?;
                        let connection_id = config.web_search_engine_connection_id.trim();
                        if connection_id.is_empty() {
                            return Err(Error::ValidationError("web_search_engine_connection_id is required".to_string()));
                        }
                        let reference = crate::storage::resource_resolver::build_web_search_engine_ref(
                            Some(connection_id), &crate::storage::load_connections()?,
                        )?.ok_or_else(|| Error::ValidationError("web_search_engine_connection_id is required".to_string()))?;
                        Ok(store_script_resource(DataValue::WebSearchEngineRef(reference)))
                    }
                    "bot.sender_from_event" => {
                        let event = message_event(params)?;
                        let sender = crate::ims_bot_adapter::runtime::models::sender_model::Sender::from_message_event(&event)
                            .ok_or_else(|| Error::ValidationError("group message is missing group_id".to_string()))?;
                        Ok(serde_json::to_value(sender)?)
                    }
                    "bot.adapter" => {
                        let config_id = required_string(params, "config_id")?;
                        let handle = crate::runtime::block_async(
                            crate::ims_bot_adapter::runtime::active_adapter_manager::ActiveAdapterManager::shared()
                                .get_active_bot_adapter_handle(&config_id),
                        )?;
                        Ok(store_script_resource(DataValue::BotAdapterRef(handle)))
                    }
                    "bot.send" => {
                        let adapter = bot_adapter_resource(params, "ims_bot_adapter")?;
                        let sender = serde_json::from_value::<crate::ims_bot_adapter::models::sender_model::Sender>(
                            params.get("sender").cloned().ok_or_else(|| Error::ValidationError("sender is required".to_string()))?,
                        ).map_err(|error| Error::ValidationError(format!("sender is invalid: {error}")))?;
                        let messages = serde_json::from_value::<Vec<crate::ims_bot_adapter::models::message::Message>>(
                            params.get("message").cloned().ok_or_else(|| Error::ValidationError("message is required".to_string()))?,
                        ).map_err(|error| Error::ValidationError(format!("message must be Vec<QQMessage>: {error}")))?;
                        let (action_name, target_id, params, target_label) = match sender {
                            crate::ims_bot_adapter::models::sender_model::Sender::Friend(friend) => {
                                let target_id = friend.user_id.to_string();
                                ("send_private_msg", target_id.clone(), json!({
                                    "user_id": target_id,
                                    "message": crate::ims_bot_adapter::runtime::ws_action::qq_message_list_to_send_json(&adapter, &messages)?,
                                }), "private")
                            }
                            crate::ims_bot_adapter::models::sender_model::Sender::Group(group) => {
                                let target_id = group.group_id.to_string();
                                ("send_group_msg", target_id.clone(), json!({
                                    "group_id": target_id,
                                    "message": crate::ims_bot_adapter::runtime::ws_action::qq_message_list_to_send_json(&adapter, &messages)?,
                                }), "group")
                            }
                        };
                        let response = crate::ims_bot_adapter::runtime::ws_action::ws_send_action(&adapter, action_name, params)?;
                        let success = crate::ims_bot_adapter::runtime::ws_action::response_success(&response);
                        let message_id = crate::ims_bot_adapter::runtime::ws_action::response_message_id(&response).unwrap_or(-1);
                        log::info!("[ScriptNode] sent {target_label} message to {target_id} success={success} message_id={message_id}");
                        Ok(json!({"success": success, "message_id": message_id}))
                    }
                    "bot.send_batches" => {
                        let adapter = bot_adapter_resource(params, "ims_bot_adapter")?;
                        let target_id = required_string(params, "target_id")?;
                        let target_type = match optional_string(params, "target_type").as_deref() {
                            Some(value) if value.eq_ignore_ascii_case("group") => "group",
                            _ => "friend",
                        };
                        let batches = serde_json::from_value::<Vec<Vec<crate::ims_bot_adapter::models::message::Message>>>(
                            params.get("message_batches").cloned().ok_or_else(|| Error::ValidationError("message_batches is required".to_string()))?,
                        ).map_err(|error| Error::ValidationError(format!("message_batches must be Vec<Vec<QQMessage>>: {error}")))?;
                        let delay_millis = match params.get("delay_millis") {
                            None | Some(Value::Null) => 0,
                            Some(value) => value.as_i64().filter(|value| *value >= 0)
                                .ok_or_else(|| Error::ValidationError("delay_millis must be a non-negative integer".to_string()))? as u64,
                        };
                        let results = crate::ims_bot_adapter::runtime::send_qq_message_batches::send_qq_message_batches_with_delay(
                            &adapter, target_type, &target_id, &batches, delay_millis, "[ScriptNode]",
                        );
                        Ok(json!({
                            "success": crate::ims_bot_adapter::runtime::send_qq_message_batches::actual_sends_all_successful(&results),
                            "summary": crate::ims_bot_adapter::runtime::send_qq_message_batches::build_send_summary(target_type, &target_id, &results),
                            "message_ids": crate::ims_bot_adapter::runtime::send_qq_message_batches::message_ids_from_results(&results),
                        }))
                    }
                    "bot.extract_messages" => {
                        let event = message_event(params)?;
                        let adapter = bot_adapter_resource(params, "ims_bot_adapter")?;
                        let target_message_id = params.get("message_id").and_then(Value::as_i64)
                            .filter(|value| *value > 0);
                        let extracted = crate::ims_bot_adapter::runtime::extract_message_from_event::extract_message_outputs(
                            &event,
                            &adapter,
                            target_message_id,
                            optional_s3_resource(params, "s3_ref")?,
                        )?;
                        Ok(json!({
                            "messages": [extracted.user_message],
                            "content": extracted.content,
                            "ref_content": extracted.ref_content,
                            "is_at_me": extracted.is_at_me,
                            "at_target_list": extracted.at_target_list,
                        }))
                    }
                    "bot.sender_id_from_event" => {
                        let event = message_event(params)?;
                        Ok(Value::String(event.sender.user_id.to_string()))
                    }
                    "bot.group_id_from_event" => {
                        let event = message_event(params)?;
                        if event.message_type != crate::ims_bot_adapter::runtime::models::event_model::MessageType::Group {
                            return Err(Error::ValidationError("message_event must be a group message".to_string()));
                        }
                        let group_id = event.group_id
                            .ok_or_else(|| Error::ValidationError("group_id is missing in group message event".to_string()))?;
                        Ok(Value::String(group_id.to_string()))
                    }
                    "bot.optional_group_id_from_event" => {
                        let event = message_event(params)?;
                        let group_id = if event.message_type == crate::ims_bot_adapter::runtime::models::event_model::MessageType::Group {
                            event.group_id.ok_or_else(|| Error::ValidationError("group_id is missing in group message event".to_string()))?.to_string()
                        } else { String::new() };
                        Ok(Value::String(group_id))
                    }
                    "bot.messages_from_event" => {
                        let event = message_event(params)?;
                        Ok(Value::Array(event.message_list.into_iter()
                            .map(|message| serde_json::to_value(message).unwrap_or(Value::Null)).collect()))
                    }
                    "bot.filter_event_type" => {
                        let event = message_event(params)?;
                        let filter_type = optional_string(params, "filter_type").unwrap_or_else(|| "private".to_string());
                        let matches = match filter_type.as_str() {
                            "group" => event.message_type == crate::ims_bot_adapter::runtime::models::event_model::MessageType::Group,
                            _ => event.message_type == crate::ims_bot_adapter::runtime::models::event_model::MessageType::Private,
                        };
                        let event = serde_json::to_value(event)?;
                        Ok(if matches { json!({"true_event": event}) } else { json!({"false_event": event}) })
                    }
                    "session.get" => {
                        let session_ref = session_resource(params)?;
                        let sender_id = required_string(params, "sender_id")?;
                        let state = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                            block_in_place(|| handle.block_on(session_ref.get_state(&sender_id)))
                        } else {
                            tokio::runtime::Runtime::new()?.block_on(session_ref.get_state(&sender_id))
                        };
                        Ok(json!({"in_session": state.in_session, "state_json": state.state_json}))
                    }
                    "session.clear" => {
                        let session_ref = session_resource(params)?;
                        let sender_id = required_string(params, "sender_id")?;
                        let cleared = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                            block_in_place(|| handle.block_on(session_ref.clear_state(&sender_id)))
                        } else {
                            tokio::runtime::Runtime::new()?.block_on(session_ref.clear_state(&sender_id))
                        };
                        Ok(Value::Bool(cleared))
                    }
                    "session.try_claim" => {
                        let session_ref = session_resource(params)?;
                        let sender_id = required_string(params, "sender_id")?;
                        let desired_state = params.get("state_json").cloned();
                        let state_ref = session_ref.clone();
                        let sender_for_claim = sender_id.clone();
                        let (state, claimed) = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                            block_in_place(|| handle.block_on(state_ref.try_claim(&sender_for_claim, desired_state)))
                        } else {
                            tokio::runtime::Runtime::new()?.block_on(state_ref.try_claim(&sender_for_claim, desired_state))
                        };
                        if claimed {
                            if let (Ok(context), Some(claim_token)) = (SESSION_CLAIM_CONTEXT.try_with(Arc::clone), state.claim_token()) {
                                context.register_claim(SessionClaim { session_ref, sender_id, claim_token });
                            }
                        }
                        Ok(json!({"claimed": claimed, "in_session": state.in_session, "state_json": state.state_json}))
                    }
                    "session.release" => {
                        let session_ref = session_resource(params)?;
                        let sender_id = required_string(params, "sender_id")?;
                        let claim_token = SESSION_CLAIM_CONTEXT.try_with(|context| {
                            let token = context.claim_token_for(&session_ref.node_id, &sender_id);
                            context.unregister_claim(&session_ref.node_id, &sender_id);
                            token
                        }).ok().flatten();
                        let released = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                            block_in_place(|| handle.block_on(session_ref.release(&sender_id, claim_token)))
                        } else {
                            tokio::runtime::Runtime::new()?.block_on(session_ref.release(&sender_id, claim_token))
                        };
                        Ok(Value::Bool(released))
                    }
                    "message_cache.append" => {
                        let cache_ref = cache_resource(params)?;
                        let sender_id = required_string(params, "sender_id")?;
                        let messages = llm_messages_from_json(
                            params.get("messages").ok_or_else(|| Error::ValidationError("messages is required".to_string()))?,
                            "messages",
                        )?;
                        cache_ref.append_messages_blocking(&sender_id, messages)?;
                        Ok(Value::Bool(true))
                    }
                    "message_cache.get" => {
                        let cache_ref = cache_resource(params)?;
                        let sender_id = required_string(params, "sender_id")?;
                        let messages = cache_ref.get_messages_blocking(&sender_id)?;
                        if messages.is_empty() {
                            Ok(params.get("fallback").cloned().unwrap_or_else(|| Value::Array(Vec::new())))
                        } else {
                            Ok(Value::Array(messages.iter().map(|message| serde_json::to_value(message).unwrap_or(Value::Null)).collect()))
                        }
                    }
                    "message_cache.set" => {
                        let cache_ref = cache_resource(params)?;
                        let sender_id = required_string(params, "sender_id")?;
                        let messages = llm_messages_from_json(
                            params.get("messages").ok_or_else(|| Error::ValidationError("messages is required".to_string()))?,
                            "messages",
                        )?;
                        cache_ref.set_messages_blocking(&sender_id, messages)?;
                        Ok(Value::Bool(true))
                    }
                    "message_cache.clear" => {
                        let cache_ref = cache_resource(params)?;
                        let sender_id = required_string(params, "sender_id")?;
                        let cleared = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                            block_in_place(|| handle.block_on(cache_ref.clear_messages(&sender_id)))?
                        } else {
                            tokio::runtime::Runtime::new()?.block_on(cache_ref.clear_messages(&sender_id))?
                        };
                        Ok(Value::Bool(cleared))
                    }
                    "model.compact_context" => {
                        let model = llm_model_resource(params, "llm_model")?;
                        let messages = llm_messages_from_json(
                            params.get("messages").ok_or_else(|| Error::ValidationError("messages is required".to_string()))?,
                            "messages",
                        )?;
                        let compact_context_length = params.get("compact_context_length")
                            .and_then(Value::as_i64)
                            .filter(|value| *value > 0)
                            .unwrap_or_default() as usize;
                        let force_compact = params.get("force_compact").and_then(Value::as_bool).unwrap_or(false);
                        let result = crate::model_inference::inference_function::compact_message::compact_context_messages(
                            &model,
                            messages,
                            compact_context_length,
                            &[],
                            force_compact,
                        );
                        Ok(json!({
                            "messages": result.messages,
                            "did_compact": result.did_compact,
                            "estimated_tokens_before": result.estimated_tokens_before,
                            "estimated_tokens_after": result.estimated_tokens_after,
                        }))
                    }
                    "model.llm_infer" => {
                        let model = llm_model_resource(params, "llm_model")?;
                        let messages = llm_messages_from_json(
                            params.get("messages").ok_or_else(|| Error::ValidationError("messages is required".to_string()))?,
                            "messages",
                        )?;
                        let response = model.inference(&crate::model_inference::llm::InferenceParam {
                            messages: &messages,
                            tools: None,
                        });
                        Ok(json!({"response": [response]}))
                    }
                    "model.create_llm_from_ref" => {
                        let llm_ref_id = required_string(params, "llm_ref_id")?;
                        let llm_ref = crate::config::llm_refs::load_llm_refs()?
                            .into_iter()
                            .find(|item| item.id == llm_ref_id || item.config_id == llm_ref_id)
                            .ok_or_else(|| Error::ValidationError(format!("llm_ref '{}' not found", llm_ref_id)))?;
                        if !llm_ref.enabled {
                            return Err(Error::ValidationError(format!("llm_ref '{}' is disabled", llm_ref.name)));
                        }
                        let config = match llm_ref.model {
                            crate::model_inference::model_config::ModelRefSpec::ChatLlm { llm } => llm,
                            crate::model_inference::model_config::ModelRefSpec::TextEmbeddingLocal { .. } => {
                                return Err(Error::ValidationError(format!("llm_ref '{}' is not a chat LLM config", llm_ref.name)));
                            }
                        };
                        let model = crate::model_inference::model_factory::build_llm(config)?;
                        Ok(store_script_resource(DataValue::LLModel(model)))
                    }
                    "embedding.infer" => {
                        let model = embedding_model_resource(params)?;
                        let text = required_string(params, "text")?;
                        let embedding = model.inference(text.trim())?;
                        let dimension = embedding.len();
                        Ok(json!({"embedding": embedding, "dimension": dimension}))
                    }
                    "embedding.batch_infer" => {
                        let model = embedding_model_resource(params)?;
                        let texts = params.get("texts").and_then(Value::as_array)
                            .ok_or_else(|| Error::ValidationError("texts must be Vec<String>".to_string()))?
                            .iter()
                            .map(|value| value.as_str().map(str::trim).filter(|text| !text.is_empty())
                                .map(ToOwned::to_owned)
                                .ok_or_else(|| Error::ValidationError("texts must contain non-blank strings".to_string())))
                            .collect::<Result<Vec<_>>>()?;
                        if texts.is_empty() {
                            return Err(Error::ValidationError("texts input must not be empty".to_string()));
                        }
                        let embeddings = model.batch_inference(&texts)?;
                        let count = embeddings.len();
                        let dimension = embeddings.first().map(Vec::len).unwrap_or_default();
                        Ok(json!({"embeddings": embeddings, "count": count, "dimension": dimension}))
                    }
                    "embedding.create_remote" => {
                        let model_name = required_string(params, "model_name")?;
                        let api_endpoint = required_string(params, "api_endpoint")?;
                        let api_key = params.get("api_key").and_then(Value::as_str)
                            .filter(|value| !value.is_empty()).map(ToOwned::to_owned);
                        let timeout_secs = params.get("timeout_secs").and_then(Value::as_i64)
                            .filter(|value| *value > 0).unwrap_or(60) as u64;
                        let retry_count = params.get("retry_count").and_then(Value::as_i64)
                            .map(|value| value.max(0) as u32).unwrap_or(2);
                        let model: Arc<dyn crate::model_inference::llm::embedding_base::EmbeddingBase> = Arc::new(
                            crate::model_inference::linalg::embedding_api::EmbeddingAPI::new(
                                model_name.trim().to_string(),
                                api_endpoint.trim().to_string(),
                                api_key,
                                std::time::Duration::from_secs(timeout_secs),
                            ).with_retry_count(retry_count),
                        );
                        Ok(store_script_resource(DataValue::EmbeddingModel(model)))
                    }
                    "embedding.create_local" => {
                        let model_name = required_string(params, "model_name")?;
                        let model: Arc<dyn crate::model_inference::llm::embedding_base::EmbeddingBase> = Arc::new(
                            crate::model_inference::nn::queued_embedding_model::QueuedEmbeddingModel::new(model_name.trim().to_string())?,
                        );
                        Ok(store_script_resource(DataValue::EmbeddingModel(model)))
                    }
                    "storage.create_mysql" => {
                        let config_id = required_string(params, "config_id")?;
                        let reference = crate::runtime::block_async(
                            crate::storage::RuntimeStorageConnectionManager::shared().get_or_create_mysql_ref(&config_id),
                        )?;
                        Ok(store_script_resource(DataValue::RdbRef(crate::data_refs::RelationalDbConnection::MySql(reference))))
                    }
                    "storage.create_redis" => {
                        let config_id = required_string(params, "config_id")?;
                        let reference = crate::storage::redis::build_redis_ref_for_connection(&config_id)?;
                        Ok(store_script_resource(DataValue::RedisRef(reference)))
                    }
                    "storage.create_sqlite" => {
                        let config_id = required_string(params, "config_id")?;
                        let reference = crate::runtime::block_async(
                            crate::storage::RuntimeStorageConnectionManager::shared().get_or_create_sqlite_ref(&config_id),
                        )?;
                        Ok(store_script_resource(DataValue::RdbRef(crate::data_refs::RelationalDbConnection::Sqlite(reference))))
                    }
                    "storage.create_s3" => {
                        let config_id = required_string(params, "config_id")?;
                        let reference = crate::runtime::block_async(
                            crate::storage::RuntimeStorageConnectionManager::shared().get_or_create_s3_ref(&config_id),
                        )?;
                        Ok(store_script_resource(DataValue::S3Ref(reference)))
                    }
                    "storage.create_weaviate" => {
                        let config_id = required_string(params, "config_id")?;
                        let reference = crate::runtime::block_async(
                            crate::storage::RuntimeStorageConnectionManager::shared().get_or_create_weaviate_ref(&config_id),
                        )?;
                        Ok(store_script_resource(DataValue::WeaviateRef(reference)))
                    }
                    "storage.user_history" => {
                        let reference = mysql_resource(params, "rdb_ref")?;
                        let sender_id = required_string(params, "sender_id")?;
                        let group_id = optional_string(params, "group_id");
                        let limit = positive_limit(params, None)?;
                        let rows = crate::graph::message_rdb_history_common::run_mysql_query(&reference, move |pool| {
                            Box::pin(async move {
                                if let Some(group_id) = group_id {
                                    sqlx::query(crate::graph::message_rdb_history_common::user_history_query(Some(&group_id)))
                                        .bind(sender_id).bind(group_id)
                                        .bind(crate::graph::message_rdb_history_common::history_query_row_limit(limit))
                                        .fetch_all(pool).await
                                } else {
                                    sqlx::query(crate::graph::message_rdb_history_common::user_history_query(None))
                                        .bind(sender_id)
                                        .bind(crate::graph::message_rdb_history_common::history_query_row_limit(limit))
                                        .fetch_all(pool).await
                                }
                            })
                        })?;
                        Ok(history_messages_json(rows, limit as usize))
                    }
                    "storage.group_history" => {
                        let reference = mysql_resource(params, "rdb_ref")?;
                        let group_id = required_string(params, "group_id")?;
                        let limit = positive_limit(params, None)?;
                        let rows = crate::graph::message_rdb_history_common::run_mysql_query(&reference, move |pool| {
                            Box::pin(async move {
                                sqlx::query(crate::graph::message_rdb_history_common::group_history_query())
                                    .bind(group_id)
                                    .bind(crate::graph::message_rdb_history_common::history_query_row_limit(limit))
                                    .fetch_all(pool).await
                            })
                        })?;
                        Ok(history_messages_json(rows, limit as usize))
                    }
                    "storage.search_messages" => {
                        let reference = mysql_resource(params, "rdb_ref")?;
                        let limit = positive_limit(params, Some(100))?;
                        let builder = crate::graph::message_rdb_history_common::SearchMessagesQueryBuilder {
                            sender_id: optional_string(params, "sender_id"),
                            group_id: optional_string(params, "group_id"),
                            contain: optional_string(params, "contain"),
                            start_time: optional_string(params, "start_time"),
                            end_time: optional_string(params, "end_time"),
                            sort_by_time_desc: params.get("sort_by_time_desc").and_then(Value::as_bool).unwrap_or(true),
                            limit,
                        };
                        let (sql, query_params) = builder.build();
                        let rows = crate::graph::message_rdb_history_common::run_mysql_query(&reference, move |pool| {
                            Box::pin(async move {
                                let mut query = sqlx::query(&sql);
                                for parameter in &query_params { query = query.bind(parameter); }
                                query.fetch_all(pool).await
                            })
                        })?;
                        Ok(history_messages_json(rows, limit as usize))
                    }
                    "storage.persist_qq_message_vectors" => {
                        let weaviate_ref = weaviate_resource(params, "weaviate_ref")?;
                        let embedding_model = embedding_model_resource(params)?;
                        let messages = qq_messages_param(params, "qq_message_list")?;
                        let success = crate::storage::persist_qq_message_list(
                            &weaviate_ref,
                            embedding_model.as_ref(),
                            &messages,
                            &required_string(params, "message_id")?,
                            &required_string(params, "sender_id")?,
                            &required_string(params, "sender_name")?,
                            optional_string(params, "group_id").as_deref(),
                            optional_string(params, "group_name").as_deref(),
                        )?;
                        Ok(Value::Bool(success))
                    }
                    "storage.persist_qq_message_rdb" => {
                        let rdb_ref = mysql_resource(params, "rdb_ref")?;
                        let messages = qq_messages_param(params, "qq_message_list")?;
                        let success = crate::graph::qq_message_list_rdb_persistence::persist_qq_message_list(
                            &rdb_ref,
                            &messages,
                            required_string(params, "message_id")?,
                            required_string(params, "sender_id")?,
                            required_string(params, "sender_name")?,
                            optional_string(params, "group_id"),
                            optional_string(params, "group_name"),
                        )?;
                        Ok(Value::Bool(success))
                    }
                    "storage.persist_image_vector" => {
                        let weaviate_ref = weaviate_resource(params, "weaviate_ref")?;
                        let embedding_model = match params.get("embedding_model") {
                            Some(value) if !value.is_null() => Some(embedding_model_resource(params)?),
                            _ => None,
                        };
                        let vector = params.get("vector").and_then(Value::as_array).map(|values| {
                            values.iter().map(|value| value.as_f64().map(|value| value as f32)
                                .ok_or_else(|| Error::ValidationError("vector must contain numbers".to_string())))
                                .collect::<Result<Vec<_>>>()
                        }).transpose()?;
                        let success = crate::storage::persist_image_record(
                            &weaviate_ref,
                            crate::storage::ImagePersistenceRequest {
                                object_storage_path: &required_string(params, "object_storage_path")?,
                                description: &required_string(params, "description")?,
                                embedding_model: embedding_model.as_deref(),
                                vector: vector.as_deref(),
                                source: optional_string(params, "source").as_deref(),
                                media_id: optional_string(params, "media_id").as_deref(),
                                original_source: optional_string(params, "original_source").as_deref(),
                                name: optional_string(params, "name").as_deref(),
                                mime_type: optional_string(params, "mime_type").as_deref(),
                            },
                        )?;
                        Ok(Value::Bool(success))
                    }
                    "storage.search_images" => {
                        let weaviate_ref = weaviate_resource(params, "weaviate_ref")?;
                        let embedding_model = embedding_model_resource(params)?;
                        let query = required_string(params, "query")?;
                        let limit = params.get("limit").and_then(Value::as_i64)
                            .filter(|value| *value > 0)
                            .ok_or_else(|| Error::ValidationError("limit must be greater than 0".to_string()))? as usize;
                        let max_distance = match params.get("max_distance") {
                            None | Some(Value::Null) => Some(crate::storage::DEFAULT_MAX_DISTANCE),
                            Some(value) => Some(value.as_f64().filter(|value| *value >= 0.0)
                                .ok_or_else(|| Error::ValidationError("max_distance must be a non-negative number".to_string()))?),
                        };
                        let target_vector = optional_string(params, "target_vector");
                        let images = crate::storage::search_images(
                            &weaviate_ref,
                            embedding_model.as_ref(),
                            &query,
                            limit,
                            max_distance,
                            target_vector.as_deref(),
                        )?;
                        Ok(json!({"images": images, "has_results": !images.is_empty()}))
                    }
                    "search.create_provider" => {
                        let config_id = required_string(params, "config_id")?;
                        let engine = crate::storage::resource_resolver::build_web_search_engine_ref(
                            Some(&config_id),
                            &crate::storage::load_connections()?,
                        )?.ok_or_else(|| Error::ValidationError("config_id is required".to_string()))?;
                        Ok(store_script_resource(DataValue::WebSearchEngineRef(engine)))
                    }
                    "search.query" => {
                        let engine = web_search_resource(params)?;
                        let query = required_string(params, "query")?;
                        let search_count = params.get("search_count").and_then(Value::as_i64)
                            .filter(|value| *value > 0)
                            .ok_or_else(|| Error::ValidationError("search_count must be greater than 0".to_string()))?;
                        let results = engine.search(query.trim(), search_count)?;
                        Ok(json!({"results": results}))
                    }
                    "search.web" => {
                        let engine = params.get("web_search_engine_ref")
                            .or_else(|| params.get("tavily_ref"))
                            .ok_or_else(|| Error::ValidationError("web_search_engine_ref is required".to_string()))?;
                        let engine = web_search_resource(&json!({"tavily_ref": engine}))?;
                        let query = optional_string(params, "query").unwrap_or_default();
                        let url = optional_string(params, "url").unwrap_or_default();
                        let count = params.get("search_count").and_then(Value::as_i64).unwrap_or(3);
                        if url.is_empty() && query.is_empty() {
                            return Err(Error::ValidationError("query 和 url 不能同时为空".to_string()));
                        }
                        let results = if !url.is_empty() {
                            engine.extract_url(&url).or_else(|_| engine.fetch_url_direct(&url))?
                        } else {
                            match engine.search(&query, count) {
                                Ok(results) => results,
                                Err(_error) if reqwest::Url::parse(&query).is_ok() => engine.fetch_url_direct(&query)?,
                                Err(error) => return Err(error),
                            }
                        };
                        Ok(json!({"results": results}))
                    }
                    _ => Err(Error::ValidationError(format!("动态脚本运行时不支持宿主调用: {method}"))),
                }
            })?;
        if let Some(error) = response.get("error").and_then(Value::as_str) {
            return Err(Error::ValidationError(format!(
                "动态脚本节点 '{}' 执行失败: {error}",
                self.definition.type_id
            )));
        }
        let outputs = response.get("outputs").and_then(Value::as_object)
            .ok_or_else(|| Error::ValidationError(format!("动态脚本节点 '{}' 输出缺少 outputs 对象", self.definition.type_id)))?;
        let declared = self.output_ports.iter()
            .map(|port| (port.name.as_str(), &port.data_type)).collect::<HashMap<_, _>>();
        let mut result = NodeOutputFlow::new();
        for (name, value) in outputs {
            let Some(data_type) = declared.get(name.as_str()) else {
                return Err(Error::ValidationError(format!("动态脚本节点 '{}' 返回了未声明输出 '{name}'", self.definition.type_id)));
            };
            let parsed = script_json_to_value(value, data_type)
                .ok_or_else(|| Error::ValidationError(format!("动态脚本节点 '{}' 输出 '{name}' 类型不匹配", self.definition.type_id)))?;
            result.insert(name.clone(), parsed);
        }
        Ok(result)
    }
}
