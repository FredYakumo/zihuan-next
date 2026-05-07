use serde_json::{json, Value};
use std::backtrace::Backtrace;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};

/// NodeType enum for distinguishing node categories
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum NodeType {
    Simple,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeConfigWidget {
    ConnectionSelect,
    ActiveBotAdapterSelect,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeConfigField {
    pub key: String,
    pub data_type: DataType,
    pub description: Option<String>,
    pub required: bool,
    pub widget: NodeConfigWidget,
    #[serde(default)]
    pub connection_kind: Option<String>,
}

impl NodeConfigField {
    pub fn new(key: impl Into<String>, data_type: DataType, widget: NodeConfigWidget) -> Self {
        Self {
            key: key.into(),
            data_type,
            description: None,
            required: true,
            widget,
            connection_kind: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn with_connection_kind(mut self, connection_kind: impl Into<String>) -> Self {
        self.connection_kind = Some(connection_kind.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub node_results: HashMap<String, HashMap<String, DataValue>>,
    pub error_node_id: Option<String>,
    pub error_message: Option<String>,
}

impl ExecutionResult {
    pub fn success(node_results: HashMap<String, HashMap<String, DataValue>>) -> Self {
        Self {
            node_results,
            error_node_id: None,
            error_message: None,
        }
    }

    pub fn with_error(
        node_results: HashMap<String, HashMap<String, DataValue>>,
        error_node_id: String,
        error_message: String,
    ) -> Self {
        Self {
            node_results,
            error_node_id: Some(error_node_id),
            error_message: Some(error_message),
        }
    }
}

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use zihuan_core::error::Result;

type OutputPool = HashMap<String, HashMap<String, DataValue>>;
type InputSourceMap = HashMap<String, HashMap<String, (String, String)>>;

pub mod brain_tool_spec;
pub mod data_value;
pub mod database;
pub mod function_graph;
pub mod graph_boundary;
pub mod graph_io;
pub mod hyperparam_store;
pub mod image_weaviate_persistence;
pub mod message_mysql_chunking;
pub mod message_mysql_get_group_history;
pub mod message_mysql_get_user_history;
pub mod message_mysql_history_common;
pub mod message_persistence;
pub mod message_restore;
pub mod object_storage;
pub mod qq_message_list_mysql_persistence;
pub mod qq_message_list_weaviate_persistence;
pub mod registry;
pub mod util;

pub type RuntimeVariableStore = Arc<RwLock<HashMap<String, DataValue>>>;

#[allow(unused_imports)]
pub use data_value::{DataType, DataValue};
#[allow(unused_imports)]
pub use graph_io::{
    ensure_positions, load_graph_definition_from_json,
    load_graph_definition_from_json_with_migration, save_graph_definition_to_json, EdgeDefinition,
    GraphPosition, LoadedGraphDefinition, NodeDefinition, NodeGraphDefinition,
};
#[allow(unused_imports)]
pub use node_macros::{node_input, node_output};
#[allow(unused_imports)]
pub use registry::build_node_graph_from_definition;

/// Node input/output ports
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub data_type: DataType,
    pub description: Option<String>,
    /// Whether this port is required, only for input ports
    pub required: bool,
    /// Whether this port is hidden from the UI (internal plumbing, not user-connectable)
    #[serde(default)]
    pub hidden: bool,
}

impl Port {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            description: None,
            required: true,
            hidden: false,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
}

/// Node trait
pub trait Node: Send + Sync {
    /// Returns the type of the node
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }
    fn id(&self) -> &str;

    fn name(&self) -> &str;

    fn description(&self) -> Option<&str> {
        None
    }

    fn input_ports(&self) -> Vec<Port>;

    fn output_ports(&self) -> Vec<Port>;

    fn has_dynamic_input_ports(&self) -> bool {
        false
    }

    fn has_dynamic_output_ports(&self) -> bool {
        false
    }

    fn config_fields(&self) -> Vec<NodeConfigField> {
        Vec::new()
    }

    /// Execute the node's main logic
    /// inputs: input port name -> data value
    /// returns: output port name -> data value
    fn execute(&mut self, inputs: HashMap<String, DataValue>)
        -> Result<HashMap<String, DataValue>>;

    /// Called once at the start of each graph execution.
    ///
    /// Nodes with run-scoped state can reset themselves here so state persists
    /// during the current execution, but not across separate runs.
    fn on_graph_start(&mut self) -> Result<()> {
        Ok(())
    }

    /// Restore node-specific configuration from parsed inline values after the
    /// graph has been loaded from JSON.
    fn apply_inline_config(&mut self, _inline_values: &HashMap<String, DataValue>) -> Result<()> {
        Ok(())
    }

    /// Inject runtime values for a function-input boundary node without forcing
    /// opaque runtime references through a JSON round-trip.
    fn set_function_runtime_values(&mut self, _values: HashMap<String, DataValue>) -> Result<()> {
        Err(zihuan_core::error::Error::ValidationError(
            "Node does not accept function runtime values".to_string(),
        ))
    }

    /// Inject a run-scoped variable store shared by the whole graph execution.
    fn set_runtime_variable_store(&mut self, _store: RuntimeVariableStore) {}

    fn to_json(&self) -> Value {
        json!({
            "id": self.id(),
            "name": self.name(),
            "description": self.description(),
            "node_type": format!("{:?}", self.node_type()),
            "input_ports": serde_json::to_value(&self.input_ports()).unwrap_or(Value::Null),
            "output_ports": serde_json::to_value(&self.output_ports()).unwrap_or(Value::Null),
        })
    }

    fn validate_inputs(&self, inputs: &HashMap<String, DataValue>) -> Result<()> {
        let input_ports = self.input_ports();

        for port in &input_ports {
            match inputs.get(&port.name) {
                Some(value) => {
                    // Validate data type
                    let actual_type = value.data_type();
                    if !port.data_type.is_compatible_with(&actual_type) {
                        return Err(zihuan_core::error::Error::ValidationError(format!(
                            "Input port '{}' expects type {}, got {}",
                            port.name, port.data_type, actual_type
                        )));
                    }
                }
                None => {
                    if port.required {
                        return Err(zihuan_core::error::Error::ValidationError(format!(
                            "Required input port '{}' is missing",
                            port.name
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_outputs(&self, outputs: &HashMap<String, DataValue>) -> Result<()> {
        let output_ports = self.output_ports();

        for port in &output_ports {
            if let Some(value) = outputs.get(&port.name) {
                let actual_type = value.data_type();
                if !port.data_type.is_compatible_with(&actual_type) {
                    return Err(zihuan_core::error::Error::ValidationError(format!(
                        "Output port '{}' expects type {}, got {}",
                        port.name, port.data_type, actual_type
                    )));
                }
            }
        }

        Ok(())
    }
}

/// NodeGraph manages multiple nodes
pub struct NodeGraph {
    pub nodes: HashMap<String, Box<dyn Node>>,
    pub inline_values: HashMap<String, HashMap<String, DataValue>>,
    runtime_variable_store: RuntimeVariableStore,
    stop_flag: Arc<AtomicBool>,
    execution_task_id: Option<String>,
    execution_callback: Option<
        Arc<dyn Fn(&str, &HashMap<String, DataValue>, &HashMap<String, DataValue>) + Send + Sync>,
    >,
    edges: Vec<EdgeDefinition>,
    definition: Option<NodeGraphDefinition>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            inline_values: HashMap::new(),
            runtime_variable_store: Arc::new(RwLock::new(HashMap::new())),
            stop_flag: Arc::new(AtomicBool::new(false)),
            execution_task_id: None,
            execution_callback: None,
            edges: Vec::new(),
            definition: None,
        }
    }

    pub fn set_execution_callback<F>(&mut self, callback: F)
    where
        F: Fn(&str, &HashMap<String, DataValue>, &HashMap<String, DataValue>)
            + Send
            + Sync
            + 'static,
    {
        self.execution_callback = Some(Arc::new(callback));
    }

    pub fn set_execution_task_id(&mut self, task_id: Option<String>) {
        self.execution_task_id = task_id;
    }

    pub fn set_edges(&mut self, edges: Vec<EdgeDefinition>) {
        self.edges = edges;
    }

    pub fn set_definition(&mut self, definition: NodeGraphDefinition) {
        self.definition = Some(definition);
        self.reset_runtime_variables_from_definition();
    }

    /// If `port_name` on `node_id` is bound to a hyperparameter, return the HP name.
    /// Used to produce a clearer error message when the HP has no value set.
    fn port_binding_hp_name(&self, node_id: &str, port_name: &str) -> Option<String> {
        self.definition
            .as_ref()?
            .nodes
            .iter()
            .find(|n| n.id == node_id)?
            .port_bindings
            .get(port_name)
            .filter(|b| b.kind == crate::graph_io::PortBindingKind::Hyperparameter)
            .map(|b| b.name.clone())
    }

    fn is_node_disabled(&self, node_id: &str) -> bool {
        self.definition
            .as_ref()
            .and_then(|def| def.nodes.iter().find(|n| n.id == node_id))
            .map(|n| n.disabled)
            .unwrap_or(false)
    }

    fn node_type_label(node: &dyn Node) -> &'static str {
        match node.node_type() {
            NodeType::Simple => "simple",
        }
    }

    fn format_debug_backtrace() -> String {
        if cfg!(debug_assertions) {
            format!("\n[DEBUG_BACKTRACE]\n{}", Backtrace::force_capture())
        } else {
            String::new()
        }
    }

    fn wrap_node_error(
        node_id: &str,
        node: &dyn Node,
        stage: &str,
        err: zihuan_core::error::Error,
    ) -> zihuan_core::error::Error {
        zihuan_core::error::Error::ValidationError(format!(
            "[NODE_ERROR:{}] Node '{}' (type='{}', category='{}', stage='{}') failed: {}{}",
            node_id,
            node.name(),
            std::any::type_name_of_val(node),
            Self::node_type_label(node),
            stage,
            err,
            Self::format_debug_backtrace(),
        ))
    }

    pub fn set_runtime_variable_store(&mut self, store: RuntimeVariableStore) {
        self.runtime_variable_store = store.clone();
        for node in self.nodes.values_mut() {
            node.set_runtime_variable_store(store.clone());
        }
    }

    fn reset_runtime_variables_from_definition(&mut self) {
        let Some(definition) = &self.definition else {
            self.runtime_variable_store.write().unwrap().clear();
            return;
        };

        let mut values = HashMap::new();
        for variable in &definition.variables {
            let Some(initial_value) = variable.initial_value.as_ref() else {
                continue;
            };
            if let Some(data_value) =
                crate::registry::json_to_data_value(initial_value, &variable.data_type)
            {
                values.insert(variable.name.clone(), data_value);
            }
        }
        *self.runtime_variable_store.write().unwrap() = values;
    }

    pub fn get_stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop_flag)
    }

    pub fn runtime_variable_store(&self) -> RuntimeVariableStore {
        self.runtime_variable_store.clone()
    }

    pub fn request_stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    pub fn reset_stop_flag(&mut self) {
        self.stop_flag.store(false, Ordering::Relaxed);
    }

    pub fn add_node(&mut self, node: Box<dyn Node>) -> Result<()> {
        let id = node.id().to_string();
        if self.nodes.contains_key(&id) {
            return Err(zihuan_core::error::Error::ValidationError(format!(
                "Node with id '{}' already exists",
                id
            )));
        }
        self.nodes.insert(id, node);
        Ok(())
    }

    fn prepare_for_execution(&mut self) -> Result<()> {
        self.stop_flag.store(false, Ordering::Relaxed);
        self.reset_runtime_variables_from_definition();

        for (node_id, node) in self.nodes.iter_mut() {
            node.set_runtime_variable_store(self.runtime_variable_store.clone());
            node.on_graph_start().map_err(|e| {
                let node_ref: &dyn Node = node.as_ref();
                zihuan_core::error::Error::ValidationError(format!(
                    "[NODE_ERROR:{}] Node '{}' (type='{}', category='{}', stage='on_graph_start') failed: {}{}",
                    node_id,
                    node_ref.name(),
                    std::any::type_name_of_val(node_ref),
                    Self::node_type_label(node_ref),
                    e,
                    Self::format_debug_backtrace(),
                ))
            })?;
        }

        Ok(())
    }
    pub fn execute(&mut self) -> Result<()> {
        self.prepare_for_execution()?;

        if !self.edges.is_empty() {
            return self.execute_with_edges();
        }

        let mut output_producers: HashMap<String, String> = HashMap::new();
        for (node_id, node) in &self.nodes {
            for port in node.output_ports() {
                if let Some(existing) = output_producers.insert(port.name.clone(), node_id.clone())
                {
                    return Err(zihuan_core::error::Error::ValidationError(format!(
                        "Output port '{}' is produced by both '{}' and '{}'",
                        port.name, existing, node_id
                    )));
                }
            }
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();

        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for (node_id, node) in &self.nodes {
            if self.is_node_disabled(node_id) {
                continue;
            }
            for port in node.input_ports() {
                if let Some(producer) = output_producers.get(&port.name) {
                    if producer != node_id {
                        dependencies
                            .entry(node_id.clone())
                            .or_default()
                            .push(producer.clone());
                        dependents
                            .entry(producer.clone())
                            .or_default()
                            .push(node_id.clone());
                        if let Some(count) = in_degree.get_mut(node_id) {
                            *count += 1;
                        }
                    }
                } else if port.required {
                    // Check if the port has an inline value
                    let has_inline = self
                        .inline_values
                        .get(node_id)
                        .map(|values| values.contains_key(&port.name))
                        .unwrap_or(false);

                    if !has_inline {
                        let msg = if let Some(hp_name) =
                            self.port_binding_hp_name(node_id, &port.name)
                        {
                            format!(
                                "Hyperparameter '{}' is bound to required port '{}' on node '{}' but has no value set",
                                hp_name, port.name, node_id
                            )
                        } else {
                            format!(
                                "Required input port '{}' for node '{}' is not bound",
                                port.name, node_id
                            )
                        };
                        return Err(zihuan_core::error::Error::ValidationError(msg));
                    }
                }
            }
        }

        let mut ready: Vec<String> = in_degree
            .iter()
            .filter_map(|(id, degree)| if *degree == 0 { Some(id.clone()) } else { None })
            .collect();
        ready.sort();

        let mut ordered: Vec<String> = Vec::with_capacity(self.nodes.len());
        while !ready.is_empty() {
            let node_id = ready.remove(0);
            ordered.push(node_id.clone());

            if let Some(next_nodes) = dependents.get(&node_id) {
                for next_id in next_nodes {
                    if let Some(count) = in_degree.get_mut(next_id) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            ready.push(next_id.clone());
                        }
                    }
                }
                ready.sort();
            }
        }

        if ordered.len() != self.nodes.len() {
            return Err(zihuan_core::error::Error::ValidationError(
                "Cycle detected in node dependencies".to_string(),
            ));
        }

        let mut data_pool: HashMap<String, DataValue> = HashMap::new();
        for node_id in ordered {
            if self.is_node_disabled(&node_id) {
                continue;
            }
            let Some(inputs) = ({
                let node = self.nodes.get(&node_id).ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_if_available(
                    node.as_ref(),
                    &data_pool,
                    &output_producers,
                    &node_id,
                    self.inline_values.get(&node_id),
                )?
            }) else {
                continue;
            };

            let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;
            let outputs = node
                .execute(inputs)
                .map_err(|e| Self::wrap_node_error(&node_id, node.as_ref(), "execute", e))?;
            for (key, value) in outputs {
                if data_pool.contains_key(&key) {
                    return Err(zihuan_core::error::Error::ValidationError(format!(
                        "Output key '{}' from node '{}' conflicts with existing data",
                        key, node_id
                    )));
                }
                data_pool.insert(key, value);
            }
        }

        Ok(())
    }

    /// Execute the graph and capture results for each node
    pub fn execute_and_capture_results(&mut self) -> ExecutionResult {
        let mut node_results: HashMap<String, HashMap<String, DataValue>> = HashMap::new();

        // Try to execute, if error occurs, return early with error info
        match self.execute_and_capture_results_internal(&mut node_results) {
            Ok(()) => ExecutionResult::success(node_results),
            Err(e) => {
                // Extract node ID from error if possible
                let error_msg = e.to_string();
                let error_node_id = self.extract_error_node_id(&error_msg);
                ExecutionResult::with_error(
                    node_results,
                    error_node_id.unwrap_or_else(|| "unknown".to_string()),
                    error_msg,
                )
            }
        }
    }

    fn extract_error_node_id(&self, error_msg: &str) -> Option<String> {
        // Try to find node ID in error message like "[NODE_ERROR:xxx]"
        if let Some(start) = error_msg.find("[NODE_ERROR:") {
            if let Some(end) = error_msg[start + 12..].find(']') {
                return Some(error_msg[start + 12..start + 12 + end].to_string());
            }
        }

        // Try to find node ID in error message like "Node 'xxx' ..."
        if let Some(start) = error_msg.find("Node '") {
            if let Some(end) = error_msg[start + 6..].find('\'') {
                return Some(error_msg[start + 6..start + 6 + end].to_string());
            }
        }
        None
    }

    fn execute_and_capture_results_internal(
        &mut self,
        node_results: &mut HashMap<String, HashMap<String, DataValue>>,
    ) -> Result<()> {
        self.prepare_for_execution()?;

        if !self.edges.is_empty() {
            return self.execute_and_capture_results_with_edges(node_results);
        }

        let mut output_producers: HashMap<String, String> = HashMap::new();
        for (node_id, node) in &self.nodes {
            for port in node.output_ports() {
                if let Some(existing) = output_producers.insert(port.name.clone(), node_id.clone())
                {
                    return Err(zihuan_core::error::Error::ValidationError(format!(
                        "Output port '{}' is produced by both '{}' and '{}'",
                        port.name, existing, node_id
                    )));
                }
            }
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();

        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for (node_id, node) in &self.nodes {
            if self.is_node_disabled(node_id) {
                continue;
            }
            for port in node.input_ports() {
                if let Some(producer) = output_producers.get(&port.name) {
                    if producer != node_id {
                        dependencies
                            .entry(node_id.clone())
                            .or_default()
                            .push(producer.clone());
                        dependents
                            .entry(producer.clone())
                            .or_default()
                            .push(node_id.clone());
                        if let Some(count) = in_degree.get_mut(node_id) {
                            *count += 1;
                        }
                    }
                } else if port.required {
                    // Check if the port has an inline value
                    let has_inline = self
                        .inline_values
                        .get(node_id)
                        .map(|values| values.contains_key(&port.name))
                        .unwrap_or(false);

                    if !has_inline {
                        let msg = if let Some(hp_name) =
                            self.port_binding_hp_name(node_id, &port.name)
                        {
                            format!(
                                "Hyperparameter '{}' is bound to required port '{}' on node '{}' but has no value set",
                                hp_name, port.name, node_id
                            )
                        } else {
                            format!(
                                "Required input port '{}' for node '{}' is not bound",
                                port.name, node_id
                            )
                        };
                        return Err(zihuan_core::error::Error::ValidationError(msg));
                    }
                }
            }
        }

        let mut ready: Vec<String> = in_degree
            .iter()
            .filter_map(|(id, degree)| if *degree == 0 { Some(id.clone()) } else { None })
            .collect();
        ready.sort();

        let mut ordered: Vec<String> = Vec::with_capacity(self.nodes.len());
        while !ready.is_empty() {
            let node_id = ready.remove(0);
            ordered.push(node_id.clone());

            if let Some(next_nodes) = dependents.get(&node_id) {
                for next_id in next_nodes {
                    if let Some(count) = in_degree.get_mut(next_id) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            ready.push(next_id.clone());
                        }
                    }
                }
                ready.sort();
            }
        }

        if ordered.len() != self.nodes.len() {
            return Err(zihuan_core::error::Error::ValidationError(
                "Cycle detected in node dependencies".to_string(),
            ));
        }

        let mut data_pool: HashMap<String, DataValue> = HashMap::new();
        for node_id in ordered {
            if self.is_node_disabled(&node_id) {
                continue;
            }
            let Some(inputs) = ({
                let node = self.nodes.get(&node_id).ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_if_available(
                    node.as_ref(),
                    &data_pool,
                    &output_producers,
                    &node_id,
                    self.inline_values.get(&node_id),
                )?
            }) else {
                continue;
            };

            let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let inputs_clone = if self.execution_callback.is_some() {
                Some(inputs.clone())
            } else {
                None
            };

            let outputs = node
                .execute(inputs.clone())
                .map_err(|e| Self::wrap_node_error(&node_id, node.as_ref(), "execute", e))?;

            if let Some(cb) = &self.execution_callback {
                if let Some(inp) = inputs_clone {
                    cb(&node_id, &inp, &outputs);
                }
            }

            let mut result = inputs;
            result.extend(outputs.iter().map(|(k, v)| (k.clone(), v.clone())));
            node_results.insert(node_id.clone(), result);

            for (key, value) in outputs {
                if data_pool.contains_key(&key) {
                    return Err(zihuan_core::error::Error::ValidationError(format!(
                        "Output key '{}' from node '{}' conflicts with existing data",
                        key, node_id
                    )));
                }
                data_pool.insert(key, value);
            }
        }

        Ok(())
    }

    fn execute_with_edges(&mut self) -> Result<()> {
        let (connected_nodes, dependents, dependencies, input_sources) = self.build_edge_maps()?;

        if connected_nodes.is_empty() {
            return Ok(());
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for (node_id, deps) in &dependencies {
            if let Some(count) = in_degree.get_mut(node_id) {
                *count += deps.len();
            }
        }

        let mut ready: Vec<String> = in_degree
            .iter()
            .filter_map(|(id, degree)| if *degree == 0 { Some(id.clone()) } else { None })
            .collect();
        ready.sort();

        let mut ordered: Vec<String> = Vec::with_capacity(self.nodes.len());
        while !ready.is_empty() {
            let node_id = ready.remove(0);
            ordered.push(node_id.clone());

            if let Some(next_nodes) = dependents.get(&node_id) {
                for next_id in next_nodes {
                    if let Some(count) = in_degree.get_mut(next_id) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            ready.push(next_id.clone());
                        }
                    }
                }
                ready.sort();
            }
        }

        if ordered.len() != self.nodes.len() {
            return Err(zihuan_core::error::Error::ValidationError(
                "Cycle detected in node dependencies".to_string(),
            ));
        }

        for node_id in &connected_nodes {
            if self.is_node_disabled(node_id) {
                continue;
            }
            let node = self.nodes.get(node_id).ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let has_inline = self.inline_values.get(node_id);
            let input_map = input_sources.get(node_id);

            for port in node.input_ports() {
                if !port.required {
                    continue;
                }
                let has_edge = input_map.and_then(|m| m.get(&port.name)).is_some();
                let has_inline_value = has_inline
                    .map(|m| m.contains_key(&port.name))
                    .unwrap_or(false);
                if !has_edge && !has_inline_value {
                    let msg = if let Some(hp_name) = self.port_binding_hp_name(node_id, &port.name)
                    {
                        format!(
                            "Hyperparameter '{}' is bound to required port '{}' on node '{}' but has no value set",
                            hp_name, port.name, node_id
                        )
                    } else {
                        format!(
                            "Required input port '{}' for node '{}' is not bound",
                            port.name, node_id
                        )
                    };
                    return Err(zihuan_core::error::Error::ValidationError(msg));
                }
            }
        }

        let mut data_pool: OutputPool = HashMap::new();
        for node_id in ordered {
            if !connected_nodes.contains(&node_id) {
                continue;
            }
            if self.is_node_disabled(&node_id) {
                continue;
            }
            let inputs = {
                let node = self.nodes.get(&node_id).ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_with_edges_if_available(
                    node.as_ref(),
                    &data_pool,
                    &input_sources,
                    &node_id,
                    self.inline_values.get(&node_id),
                )?
            };

            let Some(inputs) = inputs else {
                continue;
            };

            let inputs_clone = if self.execution_callback.is_some() {
                Some(inputs.clone())
            } else {
                None
            };
            let outputs = {
                let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                node.execute(inputs)
                    .map_err(|e| Self::wrap_node_error(&node_id, node.as_ref(), "execute", e))?
            };

            if let Some(cb) = &self.execution_callback {
                if let Some(inp) = inputs_clone {
                    cb(&node_id, &inp, &outputs);
                }
            }

            self.insert_outputs(&mut data_pool, &node_id, outputs);
        }

        Ok(())
    }

    fn execute_and_capture_results_with_edges(
        &mut self,
        node_results: &mut HashMap<String, HashMap<String, DataValue>>,
    ) -> Result<()> {
        let (connected_nodes, dependents, dependencies, input_sources) = self.build_edge_maps()?;

        if connected_nodes.is_empty() {
            return Ok(());
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for node_id in self.nodes.keys() {
            in_degree.insert(node_id.clone(), 0);
        }

        for (node_id, deps) in &dependencies {
            if let Some(count) = in_degree.get_mut(node_id) {
                *count += deps.len();
            }
        }

        let mut ready: Vec<String> = in_degree
            .iter()
            .filter_map(|(id, degree)| if *degree == 0 { Some(id.clone()) } else { None })
            .collect();
        ready.sort();

        let mut ordered: Vec<String> = Vec::with_capacity(self.nodes.len());
        while !ready.is_empty() {
            let node_id = ready.remove(0);
            ordered.push(node_id.clone());

            if let Some(next_nodes) = dependents.get(&node_id) {
                for next_id in next_nodes {
                    if let Some(count) = in_degree.get_mut(next_id) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            ready.push(next_id.clone());
                        }
                    }
                }
                ready.sort();
            }
        }

        if ordered.len() != self.nodes.len() {
            return Err(zihuan_core::error::Error::ValidationError(
                "Cycle detected in node dependencies".to_string(),
            ));
        }

        for node_id in &connected_nodes {
            if self.is_node_disabled(node_id) {
                continue;
            }
            let node = self.nodes.get(node_id).ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let has_inline = self.inline_values.get(node_id);
            let input_map = input_sources.get(node_id);

            for port in node.input_ports() {
                if !port.required {
                    continue;
                }
                let has_edge = input_map.and_then(|m| m.get(&port.name)).is_some();
                let has_inline_value = has_inline
                    .map(|m| m.contains_key(&port.name))
                    .unwrap_or(false);
                if !has_edge && !has_inline_value {
                    let msg = if let Some(hp_name) = self.port_binding_hp_name(node_id, &port.name)
                    {
                        format!(
                            "Hyperparameter '{}' is bound to required port '{}' on node '{}' but has no value set",
                            hp_name, port.name, node_id
                        )
                    } else {
                        format!(
                            "Required input port '{}' for node '{}' is not bound",
                            port.name, node_id
                        )
                    };
                    return Err(zihuan_core::error::Error::ValidationError(msg));
                }
            }
        }

        let mut data_pool: OutputPool = HashMap::new();
        for node_id in ordered {
            if !connected_nodes.contains(&node_id) {
                continue;
            }
            if self.is_node_disabled(&node_id) {
                continue;
            }
            let inputs = {
                let node = self.nodes.get(&node_id).ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_with_edges_if_available(
                    node.as_ref(),
                    &data_pool,
                    &input_sources,
                    &node_id,
                    self.inline_values.get(&node_id),
                )?
            };

            let Some(inputs) = inputs else {
                continue;
            };

            let inputs_clone = if self.execution_callback.is_some() {
                Some(inputs.clone())
            } else {
                None
            };
            let outputs = {
                let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                node.execute(inputs.clone())
                    .map_err(|e| Self::wrap_node_error(&node_id, node.as_ref(), "execute", e))?
            };

            if let Some(cb) = &self.execution_callback {
                if let Some(inp) = inputs_clone {
                    cb(&node_id, &inp, &outputs);
                }
            }

            let mut result = inputs;
            result.extend(outputs.iter().map(|(k, v)| (k.clone(), v.clone())));
            node_results.insert(node_id.clone(), result);

            self.insert_outputs(&mut data_pool, &node_id, outputs);
        }

        Ok(())
    }

    fn build_edge_maps(
        &self,
    ) -> Result<(
        HashSet<String>,
        HashMap<String, Vec<String>>,
        HashMap<String, Vec<String>>,
        InputSourceMap,
    )> {
        let mut connected_nodes: HashSet<String> = HashSet::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();
        let mut input_sources: InputSourceMap = HashMap::new();

        for edge in &self.edges {
            let from_node = self.nodes.get(&edge.from_node_id).ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(format!(
                    "Node '{}' not found for edge",
                    edge.from_node_id
                ))
            })?;
            let to_node = self.nodes.get(&edge.to_node_id).ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(format!(
                    "Node '{}' not found for edge",
                    edge.to_node_id
                ))
            })?;

            let from_port = from_node
                .output_ports()
                .into_iter()
                .find(|p| p.name == edge.from_port)
                .ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(format!(
                        "Output port '{}' not found on node '{}'",
                        edge.from_port, edge.from_node_id
                    ))
                })?;

            let to_port = to_node
                .input_ports()
                .into_iter()
                .find(|p| p.name == edge.to_port)
                .ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(format!(
                        "Input port '{}' not found on node '{}'",
                        edge.to_port, edge.to_node_id
                    ))
                })?;

            if !from_port.data_type.is_compatible_with(&to_port.data_type) {
                return Err(zihuan_core::error::Error::ValidationError(format!(
                    "端口类型不匹配：\"{}\"的输出端口\"{}\" -> \"{}\"的输入端口\"{}\" [NODE_ERROR:{}]",
                    from_node.name(), edge.from_port, to_node.name(), edge.to_port, edge.to_node_id
                )));
            }

            connected_nodes.insert(edge.from_node_id.clone());
            connected_nodes.insert(edge.to_node_id.clone());

            dependents
                .entry(edge.from_node_id.clone())
                .or_default()
                .push(edge.to_node_id.clone());
            dependencies
                .entry(edge.to_node_id.clone())
                .or_default()
                .push(edge.from_node_id.clone());

            let entry = input_sources.entry(edge.to_node_id.clone()).or_default();
            if entry.contains_key(&edge.to_port) {
                return Err(zihuan_core::error::Error::ValidationError(format!(
                    "Input port '{}' on node '{}' has multiple connections",
                    edge.to_port, edge.to_node_id
                )));
            }
            entry.insert(
                edge.to_port.clone(),
                (edge.from_node_id.clone(), edge.from_port.clone()),
            );
        }

        Ok((connected_nodes, dependents, dependencies, input_sources))
    }

    fn collect_inputs_with_edges_if_available(
        &self,
        node: &dyn Node,
        data_pool: &OutputPool,
        input_sources: &InputSourceMap,
        node_id: &str,
        inline_values: Option<&HashMap<String, DataValue>>,
    ) -> Result<Option<HashMap<String, DataValue>>> {
        let mut inputs: HashMap<String, DataValue> = HashMap::new();
        let sources = input_sources.get(node_id);

        for port in node.input_ports() {
            let bound_variable_value = self.runtime_bound_variable_value(node_id, &port.name);
            if let Some(source_map) = sources.and_then(|m| m.get(&port.name)) {
                let (from_node_id, from_port) = source_map;
                if let Some(value) = data_pool
                    .get(from_node_id)
                    .and_then(|from_outputs| from_outputs.get(from_port))
                {
                    inputs.insert(port.name.clone(), value.clone());
                    continue;
                }

                return Ok(None);
            }

            if let Some(value) = bound_variable_value {
                inputs.insert(port.name.clone(), value);
            } else if let Some(value) = inline_values.and_then(|m| m.get(&port.name)) {
                inputs.insert(port.name.clone(), value.clone());
            } else if port.required {
                return Ok(None);
            }
        }

        node.validate_inputs(&inputs)
            .map_err(|e| Self::wrap_node_error(node_id, node, "validate_inputs", e))?;
        Ok(Some(inputs))
    }

    fn insert_outputs(
        &self,
        pool: &mut OutputPool,
        node_id: &str,
        outputs: HashMap<String, DataValue>,
    ) {
        let entry = pool.entry(node_id.to_string()).or_default();
        for (key, value) in outputs {
            entry.insert(key, value);
        }
    }

    fn collect_inputs_if_available(
        &self,
        node: &dyn Node,
        data_pool: &HashMap<String, DataValue>,
        output_producers: &HashMap<String, String>,
        node_id: &str,
        inline_values: Option<&HashMap<String, DataValue>>,
    ) -> Result<Option<HashMap<String, DataValue>>> {
        let mut inputs: HashMap<String, DataValue> = HashMap::new();
        for port in node.input_ports() {
            let bound_variable_value = self.runtime_bound_variable_value(node_id, &port.name);
            if let Some(value) = data_pool.get(&port.name) {
                inputs.insert(port.name.clone(), value.clone());
            } else if output_producers.contains_key(&port.name) {
                return Ok(None);
            } else if let Some(value) = bound_variable_value {
                inputs.insert(port.name.clone(), value);
            } else if let Some(value) = inline_values.and_then(|m| m.get(&port.name)) {
                inputs.insert(port.name.clone(), value.clone());
            } else if port.required {
                return Ok(None);
            }
        }
        node.validate_inputs(&inputs)
            .map_err(|e| Self::wrap_node_error(node_id, node, "validate_inputs", e))?;
        Ok(Some(inputs))
    }

    fn runtime_bound_variable_value(&self, node_id: &str, port_name: &str) -> Option<DataValue> {
        let definition = self.definition.as_ref()?;
        let node = definition.nodes.iter().find(|node| node.id == node_id)?;
        let binding = node.port_bindings.get(port_name)?;
        if binding.kind != crate::graph_io::PortBindingKind::Variable {
            return None;
        }
        self.runtime_variable_store
            .read()
            .unwrap()
            .get(&binding.name)
            .cloned()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "nodes": self.nodes.iter().map(|(id, node)| {
                json!({
                    "id": id,
                    "node": node.to_json(),
                })
            }).collect::<Vec<_>>(),
        })
    }

    pub fn to_definition(&self) -> NodeGraphDefinition {
        NodeGraphDefinition::from_node_graph(self)
    }
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self::new()
    }
}
