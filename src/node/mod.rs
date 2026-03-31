use log::{error, info};
use serde_json::{json, Value};
use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, RwLock,
};
use tokio::task::JoinHandle;

/// NodeType enum for distinguishing node categories
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum NodeType {
    Simple,
    EventProducer,
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

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

type OutputPool = HashMap<String, HashMap<String, DataValue>>;
type InputSourceMap = HashMap<String, HashMap<String, (String, String)>>;

pub mod data_value;
pub mod database;
pub mod function_graph;
pub mod graph_io;
pub mod message_cache;
pub mod message_mysql_get_group_history;
pub mod message_mysql_get_user_history;
pub mod message_mysql_history_common;
pub mod message_nodes;
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

/// Node input/output ports
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub data_type: DataType,
    pub description: Option<String>,
    /// Whether this port is required, only for input ports
    pub required: bool,
}

impl Port {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            description: None,
            required: true,
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
        Err(crate::error::Error::ValidationError(
            "Node does not accept function runtime values".to_string(),
        ))
    }

    /// Event producer lifecycle: called before update loop
    fn on_start(&mut self, _inputs: HashMap<String, DataValue>) -> Result<()> {
        Ok(())
    }

    /// Event producer lifecycle: called repeatedly to produce outputs
    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
        Ok(None)
    }

    /// Called before the event producer loop starts. Nodes that block in
    /// on_update() should store this flag and use it to interrupt the wait.
    fn set_stop_flag(&mut self, _stop_flag: Arc<AtomicBool>) {}

    /// Inject a run-scoped variable store shared by the whole graph execution.
    fn set_runtime_variable_store(&mut self, _store: RuntimeVariableStore) {}

    /// Event producer lifecycle: called after update loop exits
    fn on_cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    /// Called when an event producer wants to stop its own loop after an error.
    /// Default is no-op; LoopNode overrides this to request break.
    fn on_error_request_stop(&self) {}

    /// Whether an event producer should stop itself quietly instead of failing
    /// the whole graph after `on_error_request_stop` is called.
    fn suppress_error_after_stop_request(&self) -> bool {
        false
    }

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
                        return Err(crate::error::Error::ValidationError(format!(
                            "Input port '{}' expects type {}, got {}",
                            port.name, port.data_type, actual_type
                        )));
                    }
                }
                None => {
                    if port.required {
                        return Err(crate::error::Error::ValidationError(format!(
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
                    return Err(crate::error::Error::ValidationError(format!(
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
    execution_callback: Option<
        Arc<dyn Fn(&str, &HashMap<String, DataValue>, &HashMap<String, DataValue>) + Send + Sync>,
    >,
    edges: Vec<EdgeDefinition>,
    definition: Option<NodeGraphDefinition>,
    event_task_runtime: Option<tokio::runtime::Runtime>,
    event_task_handles: Vec<JoinHandle<()>>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            inline_values: HashMap::new(),
            runtime_variable_store: Arc::new(RwLock::new(HashMap::new())),
            stop_flag: Arc::new(AtomicBool::new(false)),
            execution_callback: None,
            edges: Vec::new(),
            definition: None,
            event_task_runtime: None,
            event_task_handles: Vec::new(),
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

    pub fn set_edges(&mut self, edges: Vec<EdgeDefinition>) {
        self.edges = edges;
    }

    pub fn set_definition(&mut self, definition: NodeGraphDefinition) {
        self.definition = Some(definition);
        self.reset_runtime_variables_from_definition();
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
                crate::node::registry::json_to_data_value(initial_value, &variable.data_type)
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

    fn stop_current_event_producer_on_error(
        &self,
        event_producer_id: &str,
        err: &crate::error::Error,
    ) -> bool {
        let Some(node) = self.nodes.get(event_producer_id) else {
            return false;
        };

        node.on_error_request_stop();
        if node.suppress_error_after_stop_request() {
            error!(
                "Event producer '{}' stopped after internal error: {}",
                event_producer_id, err
            );
            true
        } else {
            false
        }
    }

    pub fn add_node(&mut self, node: Box<dyn Node>) -> Result<()> {
        let id = node.id().to_string();
        if self.nodes.contains_key(&id) {
            return Err(crate::error::Error::ValidationError(format!(
                "Node with id '{}' already exists",
                id
            )));
        }
        self.nodes.insert(id, node);
        Ok(())
    }

    fn prepare_for_execution(&mut self) -> Result<()> {
        self.stop_flag.store(false, Ordering::Relaxed);
        self.event_task_handles.clear();
        self.reset_runtime_variables_from_definition();

        for (node_id, node) in self.nodes.iter_mut() {
            node.set_runtime_variable_store(self.runtime_variable_store.clone());
            node.on_graph_start().map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
            })?;
        }

        Ok(())
    }

    fn block_on_future<F>(&mut self, future: F) -> Result<F::Output>
    where
        F: Future,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            Ok(tokio::task::block_in_place(|| handle.block_on(future)))
        } else {
            if self.event_task_runtime.is_none() {
                self.event_task_runtime = Some(tokio::runtime::Runtime::new()?);
            }
            Ok(self
                .event_task_runtime
                .as_mut()
                .expect("runtime should exist")
                .block_on(future))
        }
    }

    fn event_task_handle(&mut self) -> Result<tokio::runtime::Handle> {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            return Ok(handle);
        }

        if self.event_task_runtime.is_none() {
            self.event_task_runtime = Some(tokio::runtime::Runtime::new()?);
        }

        Ok(self
            .event_task_runtime
            .as_ref()
            .expect("runtime should exist")
            .handle()
            .clone())
    }

    fn wait_for_event_tasks(&mut self) -> Result<()> {
        if self.event_task_handles.is_empty() {
            return Ok(());
        }

        let handles = std::mem::take(&mut self.event_task_handles);
        self.block_on_future(async move {
            for handle in handles {
                let _ = handle.await;
            }
        })?;
        Ok(())
    }

    fn reap_finished_event_tasks(&mut self) -> Result<()> {
        if self.event_task_handles.is_empty() {
            return Ok(());
        }

        let mut pending = Vec::new();
        let mut completed = Vec::new();

        for handle in std::mem::take(&mut self.event_task_handles) {
            if handle.is_finished() {
                completed.push(handle);
            } else {
                pending.push(handle);
            }
        }

        if !completed.is_empty() {
            self.block_on_future(async move {
                for handle in completed {
                    let _ = handle.await;
                }
            })?;
        }

        self.event_task_handles = pending;
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
                    return Err(crate::error::Error::ValidationError(format!(
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
                        return Err(crate::error::Error::ValidationError(format!(
                            "Required input port '{}' for node '{}' is not bound",
                            port.name, node_id
                        )));
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
            return Err(crate::error::Error::ValidationError(
                "Cycle detected in node dependencies".to_string(),
            ));
        }

        let event_producer_set: HashSet<String> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                if node.node_type() == NodeType::EventProducer {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        if event_producer_set.is_empty() {
            let mut data_pool: HashMap<String, DataValue> = HashMap::new();
            for node_id in ordered {
                let Some(inputs) = ({
                    let node = self.nodes.get(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
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
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                let outputs = node.execute(inputs)?;
                for (key, value) in outputs {
                    if data_pool.contains_key(&key) {
                        return Err(crate::error::Error::ValidationError(format!(
                            "Output key '{}' from node '{}' conflicts with existing data",
                            key, node_id
                        )));
                    }
                    data_pool.insert(key, value);
                }
            }

            return Ok(());
        }

        let mut reachable_from_event: HashSet<String> = HashSet::new();
        let mut reachable_map: HashMap<String, HashSet<String>> = HashMap::new();
        for event_id in &event_producer_set {
            let mut visited: HashSet<String> = HashSet::new();
            let mut stack: Vec<String> = vec![event_id.clone()];
            while let Some(current) = stack.pop() {
                if !visited.insert(current.clone()) {
                    continue;
                }
                if let Some(children) = dependents.get(&current) {
                    for child in children {
                        if !visited.contains(child) {
                            stack.push(child.clone());
                        }
                    }
                }
            }
            reachable_from_event.extend(visited.iter().cloned());
            reachable_map.insert(event_id.clone(), visited);
        }

        let mut base_data_pool: HashMap<String, DataValue> = HashMap::new();
        for node_id in &ordered {
            if reachable_from_event.contains(node_id) {
                continue;
            }

            let Some(inputs) = ({
                let node = self.nodes.get(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_if_available(
                    node.as_ref(),
                    &base_data_pool,
                    &output_producers,
                    node_id,
                    self.inline_values.get(node_id),
                )?
            }) else {
                continue;
            };

            let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;
            let outputs = node.execute(inputs)?;
            for (key, value) in outputs {
                if base_data_pool.contains_key(&key) {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Output key '{}' from node '{}' conflicts with existing data",
                        key, node_id
                    )));
                }
                base_data_pool.insert(key, value);
            }
        }

        let mut event_producer_roots: Vec<String> = event_producer_set
            .iter()
            .filter(|event_id| {
                !dependencies
                    .get(*event_id)
                    .map(|deps| deps.iter().any(|dep| event_producer_set.contains(dep)))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        event_producer_roots.sort();

        for root_id in event_producer_roots {
            self.run_event_producer(
                &root_id,
                &base_data_pool,
                &output_producers,
                &reachable_map,
                &event_producer_set,
                &ordered,
            )?;
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
                    return Err(crate::error::Error::ValidationError(format!(
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
                        return Err(crate::error::Error::ValidationError(format!(
                            "Required input port '{}' for node '{}' is not bound",
                            port.name, node_id
                        )));
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
            return Err(crate::error::Error::ValidationError(
                "Cycle detected in node dependencies".to_string(),
            ));
        }

        let event_producer_set: HashSet<String> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                if node.node_type() == NodeType::EventProducer {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        if event_producer_set.is_empty() {
            let mut data_pool: HashMap<String, DataValue> = HashMap::new();
            for node_id in ordered {
                let Some(inputs) = ({
                    let node = self.nodes.get(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
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
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                let inputs_clone = if self.execution_callback.is_some() {
                    Some(inputs.clone())
                } else {
                    None
                };

                let outputs = node.execute(inputs.clone())?;

                if let Some(cb) = &self.execution_callback {
                    if let Some(inp) = inputs_clone {
                        cb(&node_id, &inp, &outputs);
                    }
                }

                // Store both inputs and outputs for this node
                let mut result = inputs;
                result.extend(outputs.iter().map(|(k, v)| (k.clone(), v.clone())));
                node_results.insert(node_id.clone(), result);

                for (key, value) in outputs {
                    if data_pool.contains_key(&key) {
                        return Err(crate::error::Error::ValidationError(format!(
                            "Output key '{}' from node '{}' conflicts with existing data",
                            key, node_id
                        )));
                    }
                    data_pool.insert(key, value);
                }
            }

            return Ok(());
        }

        // For event producers, we still need to execute but won't capture all results
        self.execute()?;

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
            return Err(crate::error::Error::ValidationError(
                "Cycle detected in node dependencies".to_string(),
            ));
        }

        for node_id in &connected_nodes {
            let node = self.nodes.get(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
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
                    return Err(crate::error::Error::ValidationError(format!(
                        "Required input port '{}' for node '{}' is not bound",
                        port.name, node_id
                    )));
                }
            }
        }

        let event_producer_set: HashSet<String> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                if node.node_type() == NodeType::EventProducer {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        if event_producer_set.is_empty() {
            let mut data_pool: OutputPool = HashMap::new();
            for node_id in ordered {
                if !connected_nodes.contains(&node_id) {
                    continue;
                }
                let inputs = {
                    let node = self.nodes.get(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
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
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            node_id
                        ))
                    })?;
                    node.execute(inputs)?
                };

                if let Some(cb) = &self.execution_callback {
                    if let Some(inp) = inputs_clone {
                        cb(&node_id, &inp, &outputs);
                    }
                }

                self.insert_outputs(&mut data_pool, &node_id, outputs);
            }

            return Ok(());
        }

        let mut reachable_from_event: HashSet<String> = HashSet::new();
        let mut reachable_map: HashMap<String, HashSet<String>> = HashMap::new();
        for event_id in &event_producer_set {
            let mut visited: HashSet<String> = HashSet::new();
            let mut stack: Vec<String> = vec![event_id.clone()];
            while let Some(current) = stack.pop() {
                if !visited.insert(current.clone()) {
                    continue;
                }
                if let Some(children) = dependents.get(&current) {
                    for child in children {
                        if !visited.contains(child) {
                            stack.push(child.clone());
                        }
                    }
                }
            }
            reachable_from_event.extend(visited.iter().cloned());
            reachable_map.insert(event_id.clone(), visited);
        }

        let mut base_data_pool: OutputPool = HashMap::new();
        for node_id in &ordered {
            if !connected_nodes.contains(node_id) {
                continue;
            }
            if reachable_from_event.contains(node_id) {
                continue;
            }

            let inputs = {
                let node = self.nodes.get(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_with_edges_if_available(
                    node.as_ref(),
                    &base_data_pool,
                    &input_sources,
                    node_id,
                    self.inline_values.get(node_id),
                )?
            };

            let Some(inputs) = inputs else {
                continue;
            };

            let outputs = {
                let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                node.execute(inputs)?
            };
            self.insert_outputs(&mut base_data_pool, node_id, outputs);
        }

        let mut event_producer_roots: Vec<String> = event_producer_set
            .iter()
            .filter(|event_id| {
                connected_nodes.contains(*event_id)
                    && !dependencies
                        .get(*event_id)
                        .map(|deps| deps.iter().any(|dep| event_producer_set.contains(dep)))
                        .unwrap_or(false)
            })
            .cloned()
            .collect();
        event_producer_roots.sort();

        for root_id in event_producer_roots {
            self.run_event_producer_with_edges(
                &root_id,
                &base_data_pool,
                &reachable_map,
                &event_producer_set,
                &ordered,
                &connected_nodes,
                &input_sources,
            )?;
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
            return Err(crate::error::Error::ValidationError(
                "Cycle detected in node dependencies".to_string(),
            ));
        }

        for node_id in &connected_nodes {
            let node = self.nodes.get(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
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
                    return Err(crate::error::Error::ValidationError(format!(
                        "Required input port '{}' for node '{}' is not bound",
                        port.name, node_id
                    )));
                }
            }
        }

        let event_producer_set: HashSet<String> = self
            .nodes
            .iter()
            .filter_map(|(id, node)| {
                if node.node_type() == NodeType::EventProducer {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        if event_producer_set.is_empty() {
            let mut data_pool: OutputPool = HashMap::new();
            for node_id in ordered {
                if !connected_nodes.contains(&node_id) {
                    continue;
                }
                let inputs = {
                    let node = self.nodes.get(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
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
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            node_id
                        ))
                    })?;
                    node.execute(inputs.clone())?
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

            return Ok(());
        }

        self.execute_with_edges()?;
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
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found for edge",
                    edge.from_node_id
                ))
            })?;
            let to_node = self.nodes.get(&edge.to_node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found for edge",
                    edge.to_node_id
                ))
            })?;

            let from_port = from_node
                .output_ports()
                .into_iter()
                .find(|p| p.name == edge.from_port)
                .ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Output port '{}' not found on node '{}'",
                        edge.from_port, edge.from_node_id
                    ))
                })?;

            let to_port = to_node
                .input_ports()
                .into_iter()
                .find(|p| p.name == edge.to_port)
                .ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Input port '{}' not found on node '{}'",
                        edge.to_port, edge.to_node_id
                    ))
                })?;

            if !from_port.data_type.is_compatible_with(&to_port.data_type) {
                return Err(crate::error::Error::ValidationError(format!(
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
                return Err(crate::error::Error::ValidationError(format!(
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

        node.validate_inputs(&inputs)?;
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

    fn spawn_event_task_with_edges(
        &mut self,
        node_id: &str,
        base_data_pool: &OutputPool,
        outputs: HashMap<String, DataValue>,
        reachable_map: &HashMap<String, HashSet<String>>,
        event_producer_set: &HashSet<String>,
        ordered: &[String],
        connected_nodes: &HashSet<String>,
        input_sources: &InputSourceMap,
        task_error: Arc<Mutex<Option<crate::error::Error>>>,
    ) -> Result<()> {
        let Some(definition) = self.definition.clone() else {
            let mut event_pool = base_data_pool.clone();
            self.insert_outputs(&mut event_pool, node_id, outputs);
            self.execute_single_event_with_edges(
                node_id,
                event_pool,
                reachable_map,
                event_producer_set,
                ordered,
                connected_nodes,
                input_sources,
            )?;
            return Ok(());
        };

        let stop_flag = Arc::clone(&self.stop_flag);
        let callback = self.execution_callback.clone();
        let node_id = node_id.to_string();
        let base_data_pool = base_data_pool.clone();
        let reachable_map = reachable_map.clone();
        let event_producer_set = event_producer_set.clone();
        let ordered = ordered.to_vec();
        let connected_nodes = connected_nodes.clone();
        let input_sources = input_sources.clone();
        let runtime_variable_store = self.runtime_variable_store.clone();
        let handle = self.event_task_handle()?;

        let join = handle.spawn(async move {
            let claim_context = Arc::new(crate::node::data_value::SessionClaimContext::default());
            crate::node::data_value::SESSION_CLAIM_CONTEXT
                .scope(claim_context.clone(), async move {
                    let result = tokio::task::block_in_place(|| -> Result<()> {
                        let mut event_graph = crate::node::registry::build_node_graph_from_definition(&definition)?;
                        if let Some(callback) = callback {
                            event_graph.execution_callback = Some(callback);
                        }
                        event_graph.set_runtime_variable_store(runtime_variable_store.clone());
                        event_graph.stop_flag = stop_flag.clone();
                        event_graph.execute_single_event_with_edges(
                            &node_id,
                            {
                                let mut event_pool = base_data_pool.clone();
                                if !outputs.is_empty() {
                                    event_pool.insert(node_id.clone(), outputs);
                                }
                                event_pool
                            },
                            &reachable_map,
                            &event_producer_set,
                            &ordered,
                            &connected_nodes,
                            &input_sources,
                        )
                    });

                    for claim in claim_context.drain_claims() {
                        info!(
                            "[NodeGraph] Auto-releasing claim after event task: session_ref={}, sender_id={}, claim_token={}",
                            claim.session_ref.node_id,
                            claim.sender_id,
                            claim.claim_token
                        );
                        let _ = claim
                            .session_ref
                            .release(&claim.sender_id, Some(claim.claim_token))
                            .await;
                    }

                    if let Err(err) = result {
                        error!("Async event task for '{}' failed: {}", node_id, err);
                        stop_flag.store(true, Ordering::Relaxed);
                        let mut guard = task_error.lock().unwrap();
                        if guard.is_none() {
                            *guard = Some(err);
                        }
                    }
                })
                .await;
        });

        self.event_task_handles.push(join);
        Ok(())
    }

    fn spawn_event_task(
        &mut self,
        node_id: &str,
        base_data_pool: &HashMap<String, DataValue>,
        outputs: HashMap<String, DataValue>,
        output_producers: &HashMap<String, String>,
        reachable_map: &HashMap<String, HashSet<String>>,
        event_producer_set: &HashSet<String>,
        ordered: &[String],
        task_error: Arc<Mutex<Option<crate::error::Error>>>,
    ) -> Result<()> {
        let Some(definition) = self.definition.clone() else {
            let mut event_pool = base_data_pool.clone();
            for (key, value) in outputs {
                event_pool.insert(key, value);
            }
            self.execute_single_event(
                node_id,
                event_pool,
                output_producers,
                reachable_map,
                event_producer_set,
                ordered,
            )?;
            return Ok(());
        };

        let stop_flag = Arc::clone(&self.stop_flag);
        let callback = self.execution_callback.clone();
        let node_id = node_id.to_string();
        let base_data_pool = base_data_pool.clone();
        let output_producers = output_producers.clone();
        let reachable_map = reachable_map.clone();
        let event_producer_set = event_producer_set.clone();
        let ordered = ordered.to_vec();
        let runtime_variable_store = self.runtime_variable_store.clone();
        let handle = self.event_task_handle()?;

        let join = handle.spawn(async move {
            let claim_context = Arc::new(crate::node::data_value::SessionClaimContext::default());
            crate::node::data_value::SESSION_CLAIM_CONTEXT
                .scope(claim_context.clone(), async move {
                    let result = tokio::task::block_in_place(|| -> Result<()> {
                        let mut event_graph = crate::node::registry::build_node_graph_from_definition(&definition)?;
                        if let Some(callback) = callback {
                            event_graph.execution_callback = Some(callback);
                        }
                        event_graph.set_runtime_variable_store(runtime_variable_store.clone());
                        event_graph.stop_flag = stop_flag.clone();
                        event_graph.execute_single_event(
                            &node_id,
                            {
                                let mut event_pool = base_data_pool.clone();
                                for (key, value) in outputs {
                                    event_pool.insert(key, value);
                                }
                                event_pool
                            },
                            &output_producers,
                            &reachable_map,
                            &event_producer_set,
                            &ordered,
                        )
                    });

                    for claim in claim_context.drain_claims() {
                        info!(
                            "[NodeGraph] Auto-releasing claim after event task: session_ref={}, sender_id={}, claim_token={}",
                            claim.session_ref.node_id,
                            claim.sender_id,
                            claim.claim_token
                        );
                        let _ = claim
                            .session_ref
                            .release(&claim.sender_id, Some(claim.claim_token))
                            .await;
                    }

                    if let Err(err) = result {
                        error!("Async event task for '{}' failed: {}", node_id, err);
                        stop_flag.store(true, Ordering::Relaxed);
                        let mut guard = task_error.lock().unwrap();
                        if guard.is_none() {
                            *guard = Some(err);
                        }
                    }
                })
                .await;
        });

        self.event_task_handles.push(join);
        Ok(())
    }

    fn execute_single_event_with_edges(
        &mut self,
        producer_node_id: &str,
        mut event_pool: OutputPool,
        reachable_map: &HashMap<String, HashSet<String>>,
        event_producer_set: &HashSet<String>,
        ordered: &[String],
        connected_nodes: &HashSet<String>,
        input_sources: &InputSourceMap,
    ) -> Result<()> {
        let reachable = reachable_map
            .get(producer_node_id)
            .cloned()
            .unwrap_or_default();
        let mut skipped: HashSet<String> = HashSet::new();

        for ordered_id in ordered {
            if ordered_id == producer_node_id {
                continue;
            }
            if skipped.contains(ordered_id)
                || !reachable.contains(ordered_id)
                || !connected_nodes.contains(ordered_id)
            {
                continue;
            }

            if event_producer_set.contains(ordered_id) {
                let ran = self.run_event_producer_with_edges(
                    ordered_id,
                    &event_pool,
                    reachable_map,
                    event_producer_set,
                    ordered,
                    connected_nodes,
                    input_sources,
                )?;
                if ran {
                    if let Some(skip_set) = reachable_map.get(ordered_id) {
                        skipped.extend(skip_set.iter().cloned());
                    }
                }
                continue;
            }

            let inputs = {
                let node = self.nodes.get(ordered_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        ordered_id
                    ))
                })?;
                self.collect_inputs_with_edges_if_available(
                    node.as_ref(),
                    &event_pool,
                    input_sources,
                    ordered_id,
                    self.inline_values.get(ordered_id),
                )?
            };

            let Some(inputs) = inputs else {
                continue;
            };

            let inputs_clone = self.execution_callback.as_ref().map(|_| inputs.clone());
            let outputs = {
                let node = self.nodes.get_mut(ordered_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        ordered_id
                    ))
                })?;
                node.execute(inputs).map_err(|e| {
                    crate::error::Error::ValidationError(format!(
                        "[NODE_ERROR:{}] {}",
                        ordered_id, e
                    ))
                })?
            };

            if let Some(cb) = &self.execution_callback {
                if let Some(inp) = inputs_clone {
                    cb(ordered_id, &inp, &outputs);
                }
            }

            self.insert_outputs(&mut event_pool, ordered_id, outputs);
        }

        Ok(())
    }

    fn execute_single_event(
        &mut self,
        producer_node_id: &str,
        mut event_pool: HashMap<String, DataValue>,
        output_producers: &HashMap<String, String>,
        reachable_map: &HashMap<String, HashSet<String>>,
        event_producer_set: &HashSet<String>,
        ordered: &[String],
    ) -> Result<()> {
        let reachable = reachable_map
            .get(producer_node_id)
            .cloned()
            .unwrap_or_default();
        let mut skipped: HashSet<String> = HashSet::new();

        for ordered_id in ordered {
            if ordered_id == producer_node_id {
                continue;
            }
            if skipped.contains(ordered_id) || !reachable.contains(ordered_id) {
                continue;
            }

            if event_producer_set.contains(ordered_id) {
                let ran = self.run_event_producer(
                    ordered_id,
                    &event_pool,
                    output_producers,
                    reachable_map,
                    event_producer_set,
                    ordered,
                )?;
                if ran {
                    if let Some(skip_set) = reachable_map.get(ordered_id) {
                        skipped.extend(skip_set.iter().cloned());
                    }
                }
                continue;
            }

            let Some(inputs) = ({
                let node = self.nodes.get(ordered_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        ordered_id
                    ))
                })?;
                self.collect_inputs_if_available(
                    node.as_ref(),
                    &event_pool,
                    output_producers,
                    ordered_id,
                    self.inline_values.get(ordered_id),
                )?
            }) else {
                continue;
            };

            let node = self.nodes.get_mut(ordered_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    ordered_id
                ))
            })?;

            let inputs_clone = self.execution_callback.as_ref().map(|_| inputs.clone());
            let outputs = node.execute(inputs).map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", ordered_id, e))
            })?;

            if let Some(cb) = &self.execution_callback {
                if let Some(inp) = inputs_clone {
                    cb(ordered_id, &inp, &outputs);
                }
            }

            for (key, value) in outputs {
                if event_pool.contains_key(&key) {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Output key '{}' from node '{}' conflicts with existing data",
                        key, ordered_id
                    )));
                }
                event_pool.insert(key, value);
            }
        }

        Ok(())
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
        node.validate_inputs(&inputs)?;
        Ok(Some(inputs))
    }

    fn runtime_bound_variable_value(&self, node_id: &str, port_name: &str) -> Option<DataValue> {
        let definition = self.definition.as_ref()?;
        let node = definition.nodes.iter().find(|node| node.id == node_id)?;
        let binding = node.port_bindings.get(port_name)?;
        if binding.kind != crate::node::graph_io::PortBindingKind::Variable {
            return None;
        }
        self.runtime_variable_store
            .read()
            .unwrap()
            .get(&binding.name)
            .cloned()
    }

    fn run_event_producer_with_edges(
        &mut self,
        node_id: &str,
        base_data_pool: &OutputPool,
        reachable_map: &HashMap<String, HashSet<String>>,
        event_producer_set: &HashSet<String>,
        ordered: &[String],
        connected_nodes: &HashSet<String>,
        input_sources: &InputSourceMap,
    ) -> Result<bool> {
        let task_error: Arc<Mutex<Option<crate::error::Error>>> = Arc::new(Mutex::new(None));
        {
            let inputs = {
                let node = self.nodes.get(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_with_edges_if_available(
                    node.as_ref(),
                    base_data_pool,
                    input_sources,
                    node_id,
                    self.inline_values.get(node_id),
                )?
            };

            let Some(inputs) = inputs else {
                return Ok(false);
            };

            let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            node.on_start(inputs).map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
            })?;
            node.set_stop_flag(Arc::clone(&self.stop_flag));
        }

        loop {
            self.reap_finished_event_tasks()?;
            if self.stop_flag.load(Ordering::Relaxed) {
                info!("Event producer '{}' stopped by user request", node_id);
                break;
            }

            let outputs = {
                let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                let update_result = node.on_update().map_err(|e| {
                    crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
                });
                let update_result = match update_result {
                    Ok(value) => value,
                    Err(err) => {
                        if self.stop_current_event_producer_on_error(node_id, &err) {
                            break;
                        }
                        return Err(err);
                    }
                };
                match update_result {
                    Some(outputs) => {
                        node.validate_outputs(&outputs)?;
                        outputs
                    }
                    None => break,
                }
            };

            if let Some(cb) = &self.execution_callback {
                cb(node_id, &HashMap::new(), &outputs);
            }
            self.spawn_event_task_with_edges(
                node_id,
                base_data_pool,
                outputs,
                reachable_map,
                event_producer_set,
                ordered,
                connected_nodes,
                input_sources,
                Arc::clone(&task_error),
            )?;
        }

        self.wait_for_event_tasks()?;

        let node = self.nodes.get_mut(node_id).ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "Node '{}' not found during cleanup",
                node_id
            ))
        })?;
        node.on_cleanup()?;

        if let Some(err) = task_error.lock().unwrap().take() {
            return Err(err);
        }

        Ok(true)
    }

    fn run_event_producer(
        &mut self,
        node_id: &str,
        base_data_pool: &HashMap<String, DataValue>,
        output_producers: &HashMap<String, String>,
        reachable_map: &HashMap<String, HashSet<String>>,
        event_producer_set: &HashSet<String>,
        ordered: &[String],
    ) -> Result<bool> {
        let task_error: Arc<Mutex<Option<crate::error::Error>>> = Arc::new(Mutex::new(None));
        {
            let Some(inputs) = ({
                let node = self.nodes.get(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;
                self.collect_inputs_if_available(
                    node.as_ref(),
                    base_data_pool,
                    output_producers,
                    node_id,
                    self.inline_values.get(node_id),
                )?
            }) else {
                return Ok(false);
            };

            let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;
            node.on_start(inputs).map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
            })?;
            node.set_stop_flag(Arc::clone(&self.stop_flag));
        }

        loop {
            self.reap_finished_event_tasks()?;
            if self.stop_flag.load(Ordering::Relaxed) {
                info!("Event producer '{}' stopped by user request", node_id);
                break;
            }

            let outputs = {
                let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                let update_result = node.on_update().map_err(|e| {
                    crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
                });
                let update_result = match update_result {
                    Ok(value) => value,
                    Err(err) => {
                        if self.stop_current_event_producer_on_error(node_id, &err) {
                            break;
                        }
                        return Err(err);
                    }
                };
                match update_result {
                    Some(outputs) => {
                        node.validate_outputs(&outputs)?;
                        outputs
                    }
                    None => break,
                }
            };

            if let Some(cb) = &self.execution_callback {
                cb(node_id, &HashMap::new(), &outputs);
            }
            self.spawn_event_task(
                node_id,
                base_data_pool,
                outputs,
                output_producers,
                reachable_map,
                event_producer_set,
                ordered,
                Arc::clone(&task_error),
            )?;
        }

        self.wait_for_event_tasks()?;

        let node = self.nodes.get_mut(node_id).ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "Node '{}' not found during cleanup",
                node_id
            ))
        })?;
        node.on_cleanup()?;

        if let Some(err) = task_error.lock().unwrap().take() {
            return Err(err);
        }

        Ok(true)
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

#[cfg(test)]
mod tests {
    use super::{
        DataType, DataValue, EdgeDefinition, ExecutionResult, Node, NodeGraph, NodeType, Port,
    };
    use crate::error::Result;
    use crate::node::graph_io::{NodeDefinition, NodeGraphDefinition};
    use crate::node::registry::NODE_REGISTRY;
    use crate::node::util::{BooleanBranchNode, SwitchNode};
    use once_cell::sync::Lazy;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct StaticOutputNode {
        id: String,
        name: String,
        output_name: String,
        value: DataValue,
    }

    impl StaticOutputNode {
        fn new(id: &str, output_name: &str, value: DataValue) -> Self {
            Self {
                id: id.to_string(),
                name: id.to_string(),
                output_name: output_name.to_string(),
                value,
            }
        }
    }

    impl Node for StaticOutputNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn input_ports(&self) -> Vec<Port> {
            Vec::new()
        }

        fn output_ports(&self) -> Vec<Port> {
            vec![Port::new(self.output_name.clone(), self.value.data_type())]
        }

        fn execute(
            &mut self,
            _inputs: HashMap<String, DataValue>,
        ) -> Result<HashMap<String, DataValue>> {
            let mut outputs = HashMap::new();
            outputs.insert(self.output_name.clone(), self.value.clone());
            Ok(outputs)
        }
    }

    struct SeenSinkNode {
        id: String,
        name: String,
        input_name: String,
        input_type: DataType,
    }

    struct OptionalSeenSinkNode {
        id: String,
        name: String,
        input_name: String,
        input_type: DataType,
    }

    static ASYNC_TEST_LOG: Lazy<Arc<Mutex<Vec<String>>>> =
        Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

    impl OptionalSeenSinkNode {
        fn new(id: &str, input_name: &str, input_type: DataType) -> Self {
            Self {
                id: id.to_string(),
                name: id.to_string(),
                input_name: input_name.to_string(),
                input_type,
            }
        }
    }

    impl SeenSinkNode {
        fn new(id: &str, input_name: &str, input_type: DataType) -> Self {
            Self {
                id: id.to_string(),
                name: id.to_string(),
                input_name: input_name.to_string(),
                input_type,
            }
        }
    }

    impl Node for SeenSinkNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn input_ports(&self) -> Vec<Port> {
            vec![Port::new(self.input_name.clone(), self.input_type.clone())]
        }

        fn output_ports(&self) -> Vec<Port> {
            vec![Port::new("seen", DataType::Boolean)]
        }

        fn execute(
            &mut self,
            inputs: HashMap<String, DataValue>,
        ) -> Result<HashMap<String, DataValue>> {
            self.validate_inputs(&inputs)?;
            Ok(HashMap::from([(
                "seen".to_string(),
                DataValue::Boolean(true),
            )]))
        }
    }

    impl Node for OptionalSeenSinkNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn input_ports(&self) -> Vec<Port> {
            vec![Port::new(self.input_name.clone(), self.input_type.clone()).optional()]
        }

        fn output_ports(&self) -> Vec<Port> {
            vec![Port::new("seen", DataType::Boolean)]
        }

        fn execute(
            &mut self,
            inputs: HashMap<String, DataValue>,
        ) -> Result<HashMap<String, DataValue>> {
            self.validate_inputs(&inputs)?;
            Ok(HashMap::from([(
                "seen".to_string(),
                DataValue::Boolean(true),
            )]))
        }
    }

    struct TestEventProducerNode {
        id: String,
        name: String,
        events: Vec<String>,
    }

    impl TestEventProducerNode {
        fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
            Self {
                id: id.into(),
                name: name.into(),
                events: vec!["user-1".to_string(), "user-2".to_string()],
            }
        }
    }

    impl Node for TestEventProducerNode {
        fn node_type(&self) -> NodeType {
            NodeType::EventProducer
        }

        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn input_ports(&self) -> Vec<Port> {
            Vec::new()
        }

        fn output_ports(&self) -> Vec<Port> {
            vec![Port::new("sender_id", DataType::String)]
        }

        fn execute(
            &mut self,
            _inputs: HashMap<String, DataValue>,
        ) -> Result<HashMap<String, DataValue>> {
            Ok(HashMap::new())
        }

        fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
            let Some(next_sender) = self.events.first().cloned() else {
                return Ok(None);
            };
            self.events.remove(0);
            Ok(Some(HashMap::from([(
                "sender_id".to_string(),
                DataValue::String(next_sender),
            )])))
        }
    }

    struct TestBlockingNode {
        id: String,
        name: String,
    }

    impl TestBlockingNode {
        fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
            Self {
                id: id.into(),
                name: name.into(),
            }
        }
    }

    impl Node for TestBlockingNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn input_ports(&self) -> Vec<Port> {
            vec![Port::new("sender_id", DataType::String)]
        }

        fn output_ports(&self) -> Vec<Port> {
            vec![Port::new("done", DataType::String)]
        }

        fn execute(
            &mut self,
            inputs: HashMap<String, DataValue>,
        ) -> Result<HashMap<String, DataValue>> {
            let sender_id = match inputs.get("sender_id") {
                Some(DataValue::String(sender_id)) => sender_id.clone(),
                other => panic!("unexpected sender_id input: {other:?}"),
            };

            ASYNC_TEST_LOG
                .lock()
                .expect("async test log should lock")
                .push(format!("start:{sender_id}"));
            if sender_id == "user-1" {
                std::thread::sleep(Duration::from_millis(200));
            }
            ASYNC_TEST_LOG
                .lock()
                .expect("async test log should lock")
                .push(format!("end:{sender_id}"));

            Ok(HashMap::from([(
                "done".to_string(),
                DataValue::String(sender_id),
            )]))
        }
    }

    fn register_async_test_nodes() {
        static REGISTERED: Lazy<()> = Lazy::new(|| {
            NODE_REGISTRY
                .register(
                    "test_event_producer_async",
                    "Test Event Producer Async",
                    "测试",
                    "Produces two sender IDs for async execution tests",
                    Arc::new(|id: String, name: String| {
                        Box::new(TestEventProducerNode::new(id, name))
                    }),
                )
                .expect("test event producer should register");
            NODE_REGISTRY
                .register(
                    "test_blocking_async",
                    "Test Blocking Async",
                    "测试",
                    "Sleeps for the first sender to validate async fan-out",
                    Arc::new(|id: String, name: String| Box::new(TestBlockingNode::new(id, name))),
                )
                .expect("test blocking node should register");
        });
        Lazy::force(&REGISTERED);
    }

    fn assert_success(result: &ExecutionResult) {
        assert!(
            result.error_message.is_none(),
            "unexpected execution error: {:?}",
            result.error_message
        );
    }

    #[test]
    fn switch_blocks_downstream_in_implicit_mode() {
        let mut graph = NodeGraph::new();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "toggle",
                "enabled",
                DataValue::Boolean(false),
            )))
            .unwrap();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "source",
                "input",
                DataValue::String("hello".to_string()),
            )))
            .unwrap();
        graph
            .add_node(Box::new(SwitchNode::new("gate", "Gate")))
            .unwrap();
        graph
            .add_node(Box::new(SeenSinkNode::new(
                "sink",
                "output",
                DataType::String,
            )))
            .unwrap();

        let result = graph.execute_and_capture_results();
        assert_success(&result);
        assert!(result.node_results.contains_key("gate"));
        assert!(!result.node_results.contains_key("sink"));
    }

    #[test]
    fn switch_blocks_downstream_in_edge_mode() {
        let mut graph = NodeGraph::new();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "toggle",
                "enabled",
                DataValue::Boolean(false),
            )))
            .unwrap();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "source",
                "value",
                DataValue::String("hello".to_string()),
            )))
            .unwrap();
        graph
            .add_node(Box::new(SwitchNode::new("gate", "Gate")))
            .unwrap();
        graph
            .add_node(Box::new(SeenSinkNode::new(
                "sink",
                "value",
                DataType::String,
            )))
            .unwrap();
        graph.set_edges(vec![
            EdgeDefinition {
                from_node_id: "toggle".to_string(),
                from_port: "enabled".to_string(),
                to_node_id: "gate".to_string(),
                to_port: "enabled".to_string(),
            },
            EdgeDefinition {
                from_node_id: "source".to_string(),
                from_port: "value".to_string(),
                to_node_id: "gate".to_string(),
                to_port: "input".to_string(),
            },
            EdgeDefinition {
                from_node_id: "gate".to_string(),
                from_port: "output".to_string(),
                to_node_id: "sink".to_string(),
                to_port: "value".to_string(),
            },
        ]);

        let result = graph.execute_and_capture_results();
        assert_success(&result);
        assert!(result.node_results.contains_key("gate"));
        assert!(!result.node_results.contains_key("sink"));
    }

    #[test]
    fn switch_blocks_optional_downstream_in_edge_mode() {
        let mut graph = NodeGraph::new();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "toggle",
                "enabled",
                DataValue::Boolean(false),
            )))
            .unwrap();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "source",
                "value",
                DataValue::Vec(Box::new(DataType::OpenAIMessage), Vec::new()),
            )))
            .unwrap();
        graph
            .add_node(Box::new(SwitchNode::new("gate", "Gate")))
            .unwrap();
        graph
            .add_node(Box::new(OptionalSeenSinkNode::new(
                "preview",
                "messages",
                DataType::Vec(Box::new(DataType::OpenAIMessage)),
            )))
            .unwrap();
        graph.set_edges(vec![
            EdgeDefinition {
                from_node_id: "toggle".to_string(),
                from_port: "enabled".to_string(),
                to_node_id: "gate".to_string(),
                to_port: "enabled".to_string(),
            },
            EdgeDefinition {
                from_node_id: "source".to_string(),
                from_port: "value".to_string(),
                to_node_id: "gate".to_string(),
                to_port: "input".to_string(),
            },
            EdgeDefinition {
                from_node_id: "gate".to_string(),
                from_port: "output".to_string(),
                to_node_id: "preview".to_string(),
                to_port: "messages".to_string(),
            },
        ]);

        let result = graph.execute_and_capture_results();
        assert_success(&result);
        assert!(result.node_results.contains_key("gate"));
        assert!(!result.node_results.contains_key("preview"));
    }

    #[test]
    fn switch_blocks_optional_downstream_in_implicit_mode() {
        let mut graph = NodeGraph::new();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "toggle",
                "enabled",
                DataValue::Boolean(false),
            )))
            .unwrap();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "source",
                "input",
                DataValue::Vec(Box::new(DataType::OpenAIMessage), Vec::new()),
            )))
            .unwrap();
        graph
            .add_node(Box::new(SwitchNode::new("gate", "Gate")))
            .unwrap();
        graph
            .add_node(Box::new(OptionalSeenSinkNode::new(
                "preview",
                "output",
                DataType::Vec(Box::new(DataType::OpenAIMessage)),
            )))
            .unwrap();

        let result = graph.execute_and_capture_results();
        assert_success(&result);
        assert!(result.node_results.contains_key("gate"));
        assert!(!result.node_results.contains_key("preview"));
    }

    #[test]
    fn switch_forwards_any_typed_values_in_edge_mode() {
        let mut graph = NodeGraph::new();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "toggle",
                "enabled",
                DataValue::Boolean(true),
            )))
            .unwrap();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "source",
                "value",
                DataValue::Json(serde_json::json!({
                    "content": "hello",
                    "is_at_me": false,
                })),
            )))
            .unwrap();
        graph
            .add_node(Box::new(SwitchNode::new("gate", "Gate")))
            .unwrap();
        graph
            .add_node(Box::new(SeenSinkNode::new("sink", "value", DataType::Json)))
            .unwrap();
        graph.set_edges(vec![
            EdgeDefinition {
                from_node_id: "toggle".to_string(),
                from_port: "enabled".to_string(),
                to_node_id: "gate".to_string(),
                to_port: "enabled".to_string(),
            },
            EdgeDefinition {
                from_node_id: "source".to_string(),
                from_port: "value".to_string(),
                to_node_id: "gate".to_string(),
                to_port: "input".to_string(),
            },
            EdgeDefinition {
                from_node_id: "gate".to_string(),
                from_port: "output".to_string(),
                to_node_id: "sink".to_string(),
                to_port: "value".to_string(),
            },
        ]);

        let result = graph.execute_and_capture_results();
        assert_success(&result);
        assert!(result.node_results.contains_key("sink"));
    }

    #[test]
    fn boolean_branch_routes_only_true_branch_in_edge_mode() {
        let mut graph = NodeGraph::new();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "cond",
                "condition",
                DataValue::Boolean(true),
            )))
            .unwrap();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "source",
                "value",
                DataValue::String("hello".to_string()),
            )))
            .unwrap();
        graph
            .add_node(Box::new(BooleanBranchNode::new("branch", "Branch")))
            .unwrap();
        graph
            .add_node(Box::new(SeenSinkNode::new(
                "true_sink",
                "value",
                DataType::String,
            )))
            .unwrap();
        graph
            .add_node(Box::new(SeenSinkNode::new(
                "false_sink",
                "value",
                DataType::String,
            )))
            .unwrap();
        graph.set_edges(vec![
            EdgeDefinition {
                from_node_id: "cond".to_string(),
                from_port: "condition".to_string(),
                to_node_id: "branch".to_string(),
                to_port: "condition".to_string(),
            },
            EdgeDefinition {
                from_node_id: "source".to_string(),
                from_port: "value".to_string(),
                to_node_id: "branch".to_string(),
                to_port: "input".to_string(),
            },
            EdgeDefinition {
                from_node_id: "branch".to_string(),
                from_port: "true_output".to_string(),
                to_node_id: "true_sink".to_string(),
                to_port: "value".to_string(),
            },
            EdgeDefinition {
                from_node_id: "branch".to_string(),
                from_port: "false_output".to_string(),
                to_node_id: "false_sink".to_string(),
                to_port: "value".to_string(),
            },
        ]);

        let result = graph.execute_and_capture_results();
        assert_success(&result);
        assert!(result.node_results.contains_key("branch"));
        assert!(result.node_results.contains_key("true_sink"));
        assert!(!result.node_results.contains_key("false_sink"));
    }

    #[test]
    fn boolean_branch_routes_only_false_branch_in_edge_mode() {
        let mut graph = NodeGraph::new();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "cond",
                "condition",
                DataValue::Boolean(false),
            )))
            .unwrap();
        graph
            .add_node(Box::new(StaticOutputNode::new(
                "source",
                "value",
                DataValue::Integer(7),
            )))
            .unwrap();
        graph
            .add_node(Box::new(BooleanBranchNode::new("branch", "Branch")))
            .unwrap();
        graph
            .add_node(Box::new(SeenSinkNode::new(
                "true_sink",
                "value",
                DataType::Integer,
            )))
            .unwrap();
        graph
            .add_node(Box::new(SeenSinkNode::new(
                "false_sink",
                "value",
                DataType::Integer,
            )))
            .unwrap();
        graph.set_edges(vec![
            EdgeDefinition {
                from_node_id: "cond".to_string(),
                from_port: "condition".to_string(),
                to_node_id: "branch".to_string(),
                to_port: "condition".to_string(),
            },
            EdgeDefinition {
                from_node_id: "source".to_string(),
                from_port: "value".to_string(),
                to_node_id: "branch".to_string(),
                to_port: "input".to_string(),
            },
            EdgeDefinition {
                from_node_id: "branch".to_string(),
                from_port: "true_output".to_string(),
                to_node_id: "true_sink".to_string(),
                to_port: "value".to_string(),
            },
            EdgeDefinition {
                from_node_id: "branch".to_string(),
                from_port: "false_output".to_string(),
                to_node_id: "false_sink".to_string(),
                to_port: "value".to_string(),
            },
        ]);

        let result = graph.execute_and_capture_results();
        assert_success(&result);
        assert!(result.node_results.contains_key("branch"));
        assert!(!result.node_results.contains_key("true_sink"));
        assert!(result.node_results.contains_key("false_sink"));
    }

    #[test]
    fn event_producer_spawns_independent_event_tasks() -> Result<()> {
        register_async_test_nodes();
        ASYNC_TEST_LOG.lock().unwrap().clear();

        let definition = NodeGraphDefinition {
            nodes: vec![
                NodeDefinition {
                    id: "producer".to_string(),
                    name: "Producer".to_string(),
                    description: None,
                    node_type: "test_event_producer_async".to_string(),
                    input_ports: Vec::new(),
                    output_ports: vec![Port::new("sender_id", DataType::String)],
                    dynamic_input_ports: false,
                    dynamic_output_ports: false,
                    position: None,
                    size: None,
                    inline_values: HashMap::new(),
                    port_bindings: HashMap::new(),
                    has_error: false,
                    has_cycle: false,
                },
                NodeDefinition {
                    id: "blocking".to_string(),
                    name: "Blocking".to_string(),
                    description: None,
                    node_type: "test_blocking_async".to_string(),
                    input_ports: vec![Port::new("sender_id", DataType::String)],
                    output_ports: vec![Port::new("done", DataType::String)],
                    dynamic_input_ports: false,
                    dynamic_output_ports: false,
                    position: None,
                    size: None,
                    inline_values: HashMap::new(),
                    port_bindings: HashMap::new(),
                    has_error: false,
                    has_cycle: false,
                },
            ],
            edges: Vec::new(),
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };

        let mut graph = crate::node::registry::build_node_graph_from_definition(&definition)?;
        graph.execute()?;

        let log = ASYNC_TEST_LOG.lock().unwrap().clone();
        let start_user_1 = log.iter().position(|entry| entry == "start:user-1");
        let start_user_2 = log.iter().position(|entry| entry == "start:user-2");
        let end_user_1 = log.iter().position(|entry| entry == "end:user-1");

        assert_eq!(log.len(), 4, "unexpected async execution log: {log:?}");
        assert!(start_user_1.is_some(), "missing first event start: {log:?}");
        assert!(
            start_user_2.is_some(),
            "missing second event start: {log:?}"
        );
        assert!(end_user_1.is_some(), "missing first event end: {log:?}");
        assert!(
            start_user_2.unwrap() < end_user_1.unwrap(),
            "second event should start before first event finishes: {log:?}"
        );

        Ok(())
    }

    #[test]
    fn graph_execution_resets_runtime_variables_to_initial_values_each_run() -> Result<()> {
        let definition = NodeGraphDefinition {
            nodes: vec![],
            edges: vec![],
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: vec![crate::node::graph_io::GraphVariable {
                name: "greeting".to_string(),
                data_type: DataType::String,
                initial_value: Some(serde_json::Value::String("hello".to_string())),
            }],
            execution_results: HashMap::new(),
        };

        let mut graph = crate::node::registry::build_node_graph_from_definition(&definition)?;
        graph
            .runtime_variable_store()
            .write()
            .unwrap()
            .insert("greeting".to_string(), DataValue::String("changed".to_string()));

        let _ = graph.execute_and_capture_results();

        let value = graph
            .runtime_variable_store()
            .read()
            .unwrap()
            .get("greeting")
            .cloned();
        assert!(matches!(value, Some(DataValue::String(value)) if value == "hello"));

        Ok(())
    }
}
