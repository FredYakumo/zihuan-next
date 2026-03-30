use serde_json::{json, Value};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread::JoinHandle;
use log::{error, info};

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

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use crate::error::Result;

type OutputPool = HashMap<String, HashMap<String, DataValue>>;
type InputSourceMap = HashMap<String, HashMap<String, (String, String)>>;
type ExecutionCallback = dyn Fn(&str, &HashMap<String, DataValue>, &HashMap<String, DataValue>) + Send + Sync;

pub mod data_value;
pub mod util;
pub mod graph_io;
pub mod registry;
pub mod database;
pub mod message_nodes;
pub mod message_cache;

#[allow(unused_imports)]
pub use data_value::{DataType, DataValue};
#[allow(unused_imports)]
pub use node_macros::{node_input, node_output};
#[allow(unused_imports)]
pub use graph_io::{
    NodeGraphDefinition,
    NodeDefinition,
    EdgeDefinition,
    GraphPosition,
    load_graph_definition_from_json,
    save_graph_definition_to_json,
    ensure_positions,
};

/// Node input/output ports
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>>;

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
                            port.name,
                            port.data_type,
                            actual_type
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
                        port.name,
                        port.data_type,
                        actual_type
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
    stop_flag: Arc<AtomicBool>,
    execution_callback: Option<Arc<ExecutionCallback>>,
    live_node_results: Arc<Mutex<HashMap<String, HashMap<String, DataValue>>>>,
    edges: Vec<EdgeDefinition>,
    source_definition: Option<Arc<NodeGraphDefinition>>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            inline_values: HashMap::new(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            execution_callback: None,
            live_node_results: Arc::new(Mutex::new(HashMap::new())),
            edges: Vec::new(),
            source_definition: None,
        }
    }

    pub fn set_execution_callback<F>(&mut self, callback: F)
    where
        F: Fn(&str, &HashMap<String, DataValue>, &HashMap<String, DataValue>) + Send + Sync + 'static,
    {
        self.execution_callback = Some(Arc::new(callback));
    }

    pub fn set_source_definition(&mut self, definition: NodeGraphDefinition) {
        self.source_definition = Some(Arc::new(definition));
    }

    pub fn set_edges(&mut self, edges: Vec<EdgeDefinition>) {
        self.edges = edges;
    }

    pub fn get_stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop_flag)
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

    fn initialize_nodes_for_execution(&mut self) -> Result<()> {
        for (node_id, node) in self.nodes.iter_mut() {
            node.on_graph_start().map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
            })?;
        }

        Ok(())
    }

    fn clear_live_node_results(&self) {
        self.live_node_results.lock().unwrap().clear();
    }

    fn snapshot_live_node_results(&self) -> HashMap<String, HashMap<String, DataValue>> {
        self.live_node_results.lock().unwrap().clone()
    }

    fn prepare_for_execution(&mut self) -> Result<()> {
        self.stop_flag.store(false, Ordering::Relaxed);
        self.clear_live_node_results();
        self.initialize_nodes_for_execution()
    }

    fn sanitize_for_recording(value: &DataValue) -> Option<DataValue> {
        match value {
            DataValue::CurrentSessionLeaseRef(_) => None,
            DataValue::Vec(inner, items) => Some(DataValue::Vec(
                inner.clone(),
                items
                    .iter()
                    .filter_map(Self::sanitize_for_recording)
                    .collect(),
            )),
            other => Some(other.clone()),
        }
    }

    fn sanitize_map_for_recording(
        values: &HashMap<String, DataValue>,
    ) -> HashMap<String, DataValue> {
        values
            .iter()
            .filter_map(|(key, value)| {
                Self::sanitize_for_recording(value).map(|sanitized| (key.clone(), sanitized))
            })
            .collect()
    }

    fn record_node_execution(
        &self,
        node_id: &str,
        inputs: &HashMap<String, DataValue>,
        outputs: &HashMap<String, DataValue>,
    ) {
        let sanitized_inputs = Self::sanitize_map_for_recording(inputs);
        let sanitized_outputs = Self::sanitize_map_for_recording(outputs);

        let mut combined = sanitized_inputs.clone();
        for (key, value) in &sanitized_outputs {
            combined.insert(key.clone(), value.clone());
        }

        self.live_node_results
            .lock()
            .unwrap()
            .insert(node_id.to_string(), combined);

        if let Some(cb) = &self.execution_callback {
            cb(node_id, &sanitized_inputs, &sanitized_outputs);
        }
    }

    pub fn execute(&mut self) -> Result<()> {
        self.prepare_for_execution()?;

        if !self.edges.is_empty() {
            return self.execute_with_edges();
        }

        let mut output_producers: HashMap<String, String> = HashMap::new();
        for (node_id, node) in &self.nodes {
            for port in node.output_ports() {
                if let Some(existing) = output_producers.insert(port.name.clone(), node_id.clone()) {
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
                        dependencies.entry(node_id.clone()).or_default().push(producer.clone());
                        dependents.entry(producer.clone()).or_default().push(node_id.clone());
                        if let Some(count) = in_degree.get_mut(node_id) {
                            *count += 1;
                        }
                    }
                } else if port.required {
                    // Check if the port has an inline value
                    let has_inline = self.inline_values
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
                let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                let Some(inputs) = Self::collect_inputs_if_available(
                    node.as_ref(),
                    &data_pool,
                    &output_producers,
                    &node_id,
                    self.inline_values.get(&node_id),
                )? else {
                    continue;
                };
                let outputs = node.execute(inputs.clone())?;
                self.record_node_execution(&node_id, &inputs, &outputs);
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

            let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let Some(inputs) = Self::collect_inputs_if_available(
                node.as_ref(),
                &base_data_pool,
                &output_producers,
                node_id,
                self.inline_values.get(node_id),
            )? else {
                continue;
            };
            let outputs = node.execute(inputs.clone())?;
            self.record_node_execution(node_id, &inputs, &outputs);
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
            if self.source_definition.is_some() {
                self.dispatch_root_event_producer(
                    &root_id,
                    &base_data_pool,
                    &output_producers,
                )?;
            } else {
                self.run_event_producer(
                    &root_id,
                    &base_data_pool,
                    &output_producers,
                    &reachable_map,
                    &event_producer_set,
                    &ordered,
                )?;
            }
        }

        Ok(())
    }

    /// Execute the graph and capture results for each node
    pub fn execute_and_capture_results(&mut self) -> ExecutionResult {
        let mut node_results: HashMap<String, HashMap<String, DataValue>> = HashMap::new();
        
        // Try to execute, if error occurs, return early with error info
        match self.execute_and_capture_results_internal(&mut node_results) {
            Ok(()) => ExecutionResult::success(self.snapshot_live_node_results()),
            Err(e) => {
                // Extract node ID from error if possible
                let error_msg = e.to_string();
                let error_node_id = self.extract_error_node_id(&error_msg);
                ExecutionResult::with_error(
                    self.snapshot_live_node_results(),
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
                if let Some(existing) = output_producers.insert(port.name.clone(), node_id.clone()) {
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
                        dependencies.entry(node_id.clone()).or_default().push(producer.clone());
                        dependents.entry(producer.clone()).or_default().push(node_id.clone());
                        if let Some(count) = in_degree.get_mut(node_id) {
                            *count += 1;
                        }
                    }
                } else if port.required {
                    // Check if the port has an inline value
                    let has_inline = self.inline_values
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
                let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        node_id
                    ))
                })?;

                let Some(inputs) = Self::collect_inputs_if_available(
                    node.as_ref(),
                    &data_pool,
                    &output_producers,
                    &node_id,
                    self.inline_values.get(&node_id),
                )? else {
                    continue;
                };
                
                let outputs = node.execute(inputs.clone())?;
                self.record_node_execution(&node_id, &inputs, &outputs);
                
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
                let has_edge = input_map
                    .and_then(|m| m.get(&port.name))
                    .is_some();
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

                let outputs = {
                    let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            node_id
                        ))
                    })?;
                    node.execute(inputs.clone())?
                };
                self.record_node_execution(&node_id, &inputs, &outputs);

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
                node.execute(inputs.clone())?
            };
            self.record_node_execution(node_id, &inputs, &outputs);
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
            if self.source_definition.is_some() {
                self.dispatch_root_event_producer_with_edges(
                    &root_id,
                    &base_data_pool,
                    &input_sources,
                )?;
            } else {
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
                let has_edge = input_map
                    .and_then(|m| m.get(&port.name))
                    .is_some();
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

                let outputs = {
                    let node = self.nodes.get_mut(&node_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            node_id
                        ))
                    })?;
                    node.execute(inputs.clone())?
                };
                self.record_node_execution(&node_id, &inputs, &outputs);

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

            if let Some(value) = inline_values.and_then(|m| m.get(&port.name)) {
                inputs.insert(port.name.clone(), value.clone());
            } else if port.required {
                return Ok(None);
            }
        }

        node.validate_inputs(&inputs)?;
        Ok(Some(inputs))
    }

    fn insert_outputs(&self, pool: &mut OutputPool, node_id: &str, outputs: HashMap<String, DataValue>) {
        let entry = pool.entry(node_id.to_string()).or_default();
        for (key, value) in outputs {
            entry.insert(key, value);
        }
    }

    fn collect_inputs_if_available(
        node: &dyn Node,
        data_pool: &HashMap<String, DataValue>,
        output_producers: &HashMap<String, String>,
        _node_id: &str,
        inline_values: Option<&HashMap<String, DataValue>>,
    ) -> Result<Option<HashMap<String, DataValue>>> {
        let mut inputs: HashMap<String, DataValue> = HashMap::new();
        for port in node.input_ports() {
            if let Some(value) = data_pool.get(&port.name) {
                inputs.insert(port.name.clone(), value.clone());
            } else if output_producers.contains_key(&port.name) {
                return Ok(None);
            } else if let Some(value) = inline_values.and_then(|m| m.get(&port.name)) {
                inputs.insert(port.name.clone(), value.clone());
            } else if port.required {
                return Ok(None);
            }
        }
        node.validate_inputs(&inputs)?;
        Ok(Some(inputs))
    }

    fn build_worker_graph_from_definition(
        definition: Arc<NodeGraphDefinition>,
        execution_callback: Option<Arc<ExecutionCallback>>,
        live_node_results: Arc<Mutex<HashMap<String, HashMap<String, DataValue>>>>,
        stop_flag: Arc<AtomicBool>,
    ) -> Result<NodeGraph> {
        let mut graph = crate::node::registry::build_node_graph_from_definition(definition.as_ref())?;
        graph.execution_callback = execution_callback;
        graph.live_node_results = live_node_results;
        graph.stop_flag = stop_flag;
        graph.source_definition = None;
        graph.initialize_nodes_for_execution()?;
        Ok(graph)
    }

    fn log_worker_result(result: std::thread::Result<Result<()>>) {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                error!("Event worker failed: {}", err);
            }
            Err(_) => {
                error!("Event worker panicked");
            }
        }
    }

    fn reap_worker_handles(handles: &mut Vec<JoinHandle<Result<()>>>) {
        let mut index = 0;
        while index < handles.len() {
            if handles[index].is_finished() {
                let handle = handles.remove(index);
                Self::log_worker_result(handle.join());
            } else {
                index += 1;
            }
        }
    }

    fn join_worker_handles(handles: Vec<JoinHandle<Result<()>>>) {
        for handle in handles {
            Self::log_worker_result(handle.join());
        }
    }

    fn execute_from_root_event_with_edges(
        &mut self,
        root_id: &str,
        base_data_pool: &OutputPool,
        root_outputs: HashMap<String, DataValue>,
    ) -> Result<()> {
        let (connected_nodes, dependents, dependencies, input_sources) = self.build_edge_maps()?;

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
            reachable_map.insert(event_id.clone(), visited);
        }

        let reachable = reachable_map
            .get(root_id)
            .cloned()
            .unwrap_or_default();

        let mut event_pool = base_data_pool.clone();
        self.insert_outputs(&mut event_pool, root_id, root_outputs);

        let mut skipped: HashSet<String> = HashSet::new();
        for ordered_id in &ordered {
            if ordered_id == root_id {
                continue;
            }
            if skipped.contains(ordered_id) {
                continue;
            }
            if !reachable.contains(ordered_id) {
                continue;
            }
            if !connected_nodes.contains(ordered_id) {
                continue;
            }

            if event_producer_set.contains(ordered_id) {
                let ran = self.run_event_producer_with_edges(
                    ordered_id,
                    &event_pool,
                    &reachable_map,
                    &event_producer_set,
                    &ordered,
                    &connected_nodes,
                    &input_sources,
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
                    &input_sources,
                    ordered_id,
                    self.inline_values.get(ordered_id),
                )?
            };

            let Some(inputs) = inputs else {
                continue;
            };

            let outputs = {
                let node = self.nodes.get_mut(ordered_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        ordered_id
                    ))
                })?;
                node.execute(inputs.clone()).map_err(|e| {
                    crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", ordered_id, e))
                })?
            };

            self.record_node_execution(ordered_id, &inputs, &outputs);
            self.insert_outputs(&mut event_pool, ordered_id, outputs);
        }

        Ok(())
    }

    fn execute_from_root_event_implicit(
        &mut self,
        root_id: &str,
        base_data_pool: &HashMap<String, DataValue>,
        root_outputs: HashMap<String, DataValue>,
    ) -> Result<()> {
        let mut output_producers: HashMap<String, String> = HashMap::new();
        for (node_id, node) in &self.nodes {
            for port in node.output_ports() {
                if let Some(existing) = output_producers.insert(port.name.clone(), node_id.clone()) {
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
                        dependencies.entry(node_id.clone()).or_default().push(producer.clone());
                        dependents.entry(producer.clone()).or_default().push(node_id.clone());
                        if let Some(count) = in_degree.get_mut(node_id) {
                            *count += 1;
                        }
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
            reachable_map.insert(event_id.clone(), visited);
        }

        let reachable = reachable_map
            .get(root_id)
            .cloned()
            .unwrap_or_default();

        let mut event_pool = base_data_pool.clone();
        for (key, value) in root_outputs {
            event_pool.insert(key, value);
        }

        let mut skipped: HashSet<String> = HashSet::new();
        for ordered_id in &ordered {
            if ordered_id == root_id {
                continue;
            }
            if skipped.contains(ordered_id) {
                continue;
            }
            if !reachable.contains(ordered_id) {
                continue;
            }

            if event_producer_set.contains(ordered_id) {
                let ran = self.run_event_producer(
                    ordered_id,
                    &event_pool,
                    &output_producers,
                    &reachable_map,
                    &event_producer_set,
                    &ordered,
                )?;
                if ran {
                    if let Some(skip_set) = reachable_map.get(ordered_id) {
                        skipped.extend(skip_set.iter().cloned());
                    }
                }
                continue;
            }

            let node = self.nodes.get_mut(ordered_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    ordered_id
                ))
            })?;

            let Some(inputs) = Self::collect_inputs_if_available(
                node.as_ref(),
                &event_pool,
                &output_producers,
                ordered_id,
                self.inline_values.get(ordered_id),
            )? else {
                continue;
            };

            let outputs = node.execute(inputs.clone()).map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", ordered_id, e))
            })?;
            self.record_node_execution(&ordered_id, &inputs, &outputs);

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

    fn dispatch_root_event_producer_with_edges(
        &mut self,
        node_id: &str,
        base_data_pool: &OutputPool,
        input_sources: &InputSourceMap,
    ) -> Result<bool> {
        let definition = self.source_definition.clone().ok_or_else(|| {
            crate::error::Error::ValidationError(
                "concurrent event dispatch requires source graph definition".to_string(),
            )
        })?;

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

        let mut worker_handles: Vec<JoinHandle<Result<()>>> = Vec::new();
        loop {
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

            self.record_node_execution(node_id, &HashMap::new(), &outputs);

            let worker_definition = definition.clone();
            let worker_callback = self.execution_callback.clone();
            let worker_live_results = Arc::clone(&self.live_node_results);
            let worker_stop_flag = Arc::clone(&self.stop_flag);
            let worker_root_id = node_id.to_string();
            let worker_base_data_pool = base_data_pool.clone();
            let worker_outputs = outputs;

            worker_handles.push(std::thread::spawn(move || {
                let mut worker_graph = Self::build_worker_graph_from_definition(
                    worker_definition,
                    worker_callback,
                    worker_live_results,
                    worker_stop_flag,
                )?;
                worker_graph.execute_from_root_event_with_edges(
                    &worker_root_id,
                    &worker_base_data_pool,
                    worker_outputs,
                )
            }));
            Self::reap_worker_handles(&mut worker_handles);
        }

        Self::join_worker_handles(worker_handles);

        let node = self.nodes.get_mut(node_id).ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "Node '{}' not found during cleanup",
                node_id
            ))
        })?;
        node.on_cleanup()?;

        Ok(true)
    }

    fn dispatch_root_event_producer(
        &mut self,
        node_id: &str,
        base_data_pool: &HashMap<String, DataValue>,
        output_producers: &HashMap<String, String>,
    ) -> Result<bool> {
        let definition = self.source_definition.clone().ok_or_else(|| {
            crate::error::Error::ValidationError(
                "concurrent event dispatch requires source graph definition".to_string(),
            )
        })?;

        {
            let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let Some(inputs) = Self::collect_inputs_if_available(
                node.as_ref(),
                base_data_pool,
                output_producers,
                node_id,
                self.inline_values.get(node_id),
            )? else {
                return Ok(false);
            };
            node.on_start(inputs).map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
            })?;
            node.set_stop_flag(Arc::clone(&self.stop_flag));
        }

        let mut worker_handles: Vec<JoinHandle<Result<()>>> = Vec::new();
        loop {
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

            self.record_node_execution(node_id, &HashMap::new(), &outputs);

            let worker_definition = definition.clone();
            let worker_callback = self.execution_callback.clone();
            let worker_live_results = Arc::clone(&self.live_node_results);
            let worker_stop_flag = Arc::clone(&self.stop_flag);
            let worker_root_id = node_id.to_string();
            let worker_base_data_pool = base_data_pool.clone();
            let worker_outputs = outputs;

            worker_handles.push(std::thread::spawn(move || {
                let mut worker_graph = Self::build_worker_graph_from_definition(
                    worker_definition,
                    worker_callback,
                    worker_live_results,
                    worker_stop_flag,
                )?;
                worker_graph.execute_from_root_event_implicit(
                    &worker_root_id,
                    &worker_base_data_pool,
                    worker_outputs,
                )
            }));
            Self::reap_worker_handles(&mut worker_handles);
        }

        Self::join_worker_handles(worker_handles);

        let node = self.nodes.get_mut(node_id).ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "Node '{}' not found during cleanup",
                node_id
            ))
        })?;
        node.on_cleanup()?;

        Ok(true)
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
        let reachable = reachable_map
            .get(node_id)
            .cloned()
            .unwrap_or_default();

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

            self.record_node_execution(node_id, &HashMap::new(), &outputs);

            let mut event_pool = base_data_pool.clone();
            self.insert_outputs(&mut event_pool, node_id, outputs);

            let mut skipped: HashSet<String> = HashSet::new();
            for ordered_id in ordered {
                if ordered_id == node_id {
                    continue;
                }
                if skipped.contains(ordered_id) {
                    continue;
                }
                if !reachable.contains(ordered_id) {
                    continue;
                }
                if !connected_nodes.contains(ordered_id) {
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

                let outputs = {
                    let node = self.nodes.get_mut(ordered_id).ok_or_else(|| {
                        crate::error::Error::ValidationError(format!(
                            "Node '{}' not found during execution",
                            ordered_id
                        ))
                    })?;
                    let exec_result = node.execute(inputs.clone()).map_err(|e| {
                        crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", ordered_id, e))
                    });
                    match exec_result {
                        Ok(value) => value,
                        Err(err) => {
                            if self.stop_current_event_producer_on_error(node_id, &err) {
                                break;
                            }
                            return Err(err);
                        }
                    }
                };
                self.record_node_execution(ordered_id, &inputs, &outputs);

                self.insert_outputs(&mut event_pool, ordered_id, outputs);
            }
        }

        let node = self.nodes.get_mut(node_id).ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "Node '{}' not found during cleanup",
                node_id
            ))
        })?;
        node.on_cleanup()?;

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
        let reachable = reachable_map
            .get(node_id)
            .cloned()
            .unwrap_or_default();

        {
            let node = self.nodes.get_mut(node_id).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node '{}' not found during execution",
                    node_id
                ))
            })?;

            let Some(inputs) = Self::collect_inputs_if_available(
                node.as_ref(),
                base_data_pool,
                output_producers,
                node_id,
                self.inline_values.get(node_id),
            )? else {
                return Ok(false);
            };
            node.on_start(inputs).map_err(|e| {
                crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", node_id, e))
            })?;
            node.set_stop_flag(Arc::clone(&self.stop_flag));
        }

        loop {
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

            self.record_node_execution(node_id, &HashMap::new(), &outputs);

            let mut event_pool = base_data_pool.clone();
            for (key, value) in outputs {
                event_pool.insert(key, value);
            }

            let mut skipped: HashSet<String> = HashSet::new();
            for ordered_id in ordered {
                if ordered_id == node_id {
                    continue;
                }
                if skipped.contains(ordered_id) {
                    continue;
                }
                if !reachable.contains(ordered_id) {
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

                let node = self.nodes.get_mut(ordered_id).ok_or_else(|| {
                    crate::error::Error::ValidationError(format!(
                        "Node '{}' not found during execution",
                        ordered_id
                    ))
                })?;

                let Some(inputs) = Self::collect_inputs_if_available(
                    node.as_ref(),
                    &event_pool,
                    output_producers,
                    ordered_id,
                    self.inline_values.get(ordered_id),
                )? else {
                    continue;
                };
                
                let exec_result = node.execute(inputs.clone()).map_err(|e| {
                    crate::error::Error::ValidationError(format!("[NODE_ERROR:{}] {}", ordered_id, e))
                });
                let outputs = match exec_result {
                    Ok(value) => value,
                    Err(err) => {
                        if self.stop_current_event_producer_on_error(node_id, &err) {
                            break;
                        }
                        return Err(err);
                    }
                };
                self.record_node_execution(ordered_id, &inputs, &outputs);

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
        }

        let node = self.nodes.get_mut(node_id).ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "Node '{}' not found during cleanup",
                node_id
            ))
        })?;
        node.on_cleanup()?;

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
    use super::{DataType, DataValue, EdgeDefinition, ExecutionResult, Node, NodeGraph, NodeType, Port};
    use crate::error::Result;
    use crate::node::graph_io::{load_graph_definition_from_json, NodeGraphDefinition};
    use crate::node::registry::{build_node_graph_from_definition, init_node_registry, NODE_REGISTRY};
    use crate::node::util::SwitchNode;
    use once_cell::sync::Lazy;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, Once};
    use std::thread;
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

        fn execute(&mut self, _inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
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

        fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
            self.validate_inputs(&inputs)?;
            Ok(HashMap::from([("seen".to_string(), DataValue::Boolean(true))]))
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

        fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
            self.validate_inputs(&inputs)?;
            Ok(HashMap::from([("seen".to_string(), DataValue::Boolean(true))]))
        }
    }

    fn assert_success(result: &ExecutionResult) {
        assert!(result.error_message.is_none(), "unexpected execution error: {:?}", result.error_message);
    }

    #[derive(Debug, Default)]
    struct DispatchStats {
        in_flight: usize,
        max_in_flight: usize,
        seen_sender_ids: Vec<String>,
    }

    static TEST_DISPATCH_STATS: Lazy<Mutex<DispatchStats>> =
        Lazy::new(|| Mutex::new(DispatchStats::default()));
    static TEST_DISPATCH_NODE_REGISTRATION: Once = Once::new();

    fn ensure_test_dispatch_nodes_registered() {
        TEST_DISPATCH_NODE_REGISTRATION.call_once(|| {
            NODE_REGISTRY
                .register(
                    "__test_full_concurrent_event_producer",
                    "Test Concurrent Producer",
                    "test",
                    "test root event producer",
                    Arc::new(|id: String, name: String| {
                        Box::new(TestConcurrentEventProducerNode::new(id, name))
                    }),
                )
                .unwrap();
            NODE_REGISTRY
                .register(
                    "__test_full_concurrent_sleep_sink",
                    "Test Concurrent Sleep Sink",
                    "test",
                    "test sink that records in-flight worker count",
                    Arc::new(|id: String, name: String| {
                        Box::new(TestConcurrentSleepSinkNode::new(id, name))
                    }),
                )
                .unwrap();
        });
    }

    fn reset_dispatch_stats() {
        *TEST_DISPATCH_STATS.lock().unwrap() = DispatchStats::default();
    }

    struct TestConcurrentEventProducerNode {
        id: String,
        name: String,
        emitted: usize,
    }

    impl TestConcurrentEventProducerNode {
        fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
            Self {
                id: id.into(),
                name: name.into(),
                emitted: 0,
            }
        }
    }

    impl Node for TestConcurrentEventProducerNode {
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

        fn execute(&mut self, _inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
            Ok(HashMap::new())
        }

        fn on_start(&mut self, _inputs: HashMap<String, DataValue>) -> Result<()> {
            self.emitted = 0;
            Ok(())
        }

        fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
            if self.emitted >= 2 {
                return Ok(None);
            }

            self.emitted += 1;
            Ok(Some(HashMap::from([(
                "sender_id".to_string(),
                DataValue::String("same-sender".to_string()),
            )])))
        }
    }

    struct TestConcurrentSleepSinkNode {
        id: String,
        name: String,
    }

    impl TestConcurrentSleepSinkNode {
        fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
            Self {
                id: id.into(),
                name: name.into(),
            }
        }
    }

    impl Node for TestConcurrentSleepSinkNode {
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
            vec![Port::new("done", DataType::Boolean)]
        }

        fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
            self.validate_inputs(&inputs)?;

            let sender_id = match inputs.get("sender_id") {
                Some(DataValue::String(sender_id)) => sender_id.clone(),
                other => panic!("unexpected sender_id input: {other:?}"),
            };

            {
                let mut stats = TEST_DISPATCH_STATS.lock().unwrap();
                stats.in_flight += 1;
                stats.max_in_flight = stats.max_in_flight.max(stats.in_flight);
                stats.seen_sender_ids.push(sender_id);
            }

            thread::sleep(Duration::from_millis(150));

            {
                let mut stats = TEST_DISPATCH_STATS.lock().unwrap();
                stats.in_flight = stats.in_flight.saturating_sub(1);
            }

            Ok(HashMap::from([("done".to_string(), DataValue::Boolean(true))]))
        }
    }

    #[test]
    fn switch_blocks_downstream_in_implicit_mode() {
        let mut graph = NodeGraph::new();
        graph.add_node(Box::new(StaticOutputNode::new("toggle", "enabled", DataValue::Boolean(false)))).unwrap();
        graph.add_node(Box::new(StaticOutputNode::new("source", "input", DataValue::String("hello".to_string())))).unwrap();
        graph.add_node(Box::new(SwitchNode::new("gate", "Gate"))).unwrap();
        graph.add_node(Box::new(SeenSinkNode::new("sink", "output", DataType::String))).unwrap();

        let result = graph.execute_and_capture_results();
        assert_success(&result);
        assert!(result.node_results.contains_key("gate"));
        assert!(!result.node_results.contains_key("sink"));
    }

    #[test]
    fn switch_blocks_downstream_in_edge_mode() {
        let mut graph = NodeGraph::new();
        graph.add_node(Box::new(StaticOutputNode::new("toggle", "enabled", DataValue::Boolean(false)))).unwrap();
        graph.add_node(Box::new(StaticOutputNode::new("source", "value", DataValue::String("hello".to_string())))).unwrap();
        graph.add_node(Box::new(SwitchNode::new("gate", "Gate"))).unwrap();
        graph.add_node(Box::new(SeenSinkNode::new("sink", "value", DataType::String))).unwrap();
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
        graph.add_node(Box::new(StaticOutputNode::new("toggle", "enabled", DataValue::Boolean(false)))).unwrap();
        graph.add_node(Box::new(StaticOutputNode::new("source", "value", DataValue::Vec(Box::new(DataType::OpenAIMessage), Vec::new())))).unwrap();
        graph.add_node(Box::new(SwitchNode::new("gate", "Gate"))).unwrap();
        graph.add_node(Box::new(OptionalSeenSinkNode::new("preview", "messages", DataType::Vec(Box::new(DataType::OpenAIMessage))))).unwrap();
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
        graph.add_node(Box::new(StaticOutputNode::new("toggle", "enabled", DataValue::Boolean(false)))).unwrap();
        graph.add_node(Box::new(StaticOutputNode::new("source", "input", DataValue::Vec(Box::new(DataType::OpenAIMessage), Vec::new())))).unwrap();
        graph.add_node(Box::new(SwitchNode::new("gate", "Gate"))).unwrap();
        graph.add_node(Box::new(OptionalSeenSinkNode::new("preview", "output", DataType::Vec(Box::new(DataType::OpenAIMessage))))).unwrap();

        let result = graph.execute_and_capture_results();
        assert_success(&result);
        assert!(result.node_results.contains_key("gate"));
        assert!(!result.node_results.contains_key("preview"));
    }

    #[test]
    fn switch_forwards_any_typed_values_in_edge_mode() {
        let mut graph = NodeGraph::new();
        graph.add_node(Box::new(StaticOutputNode::new("toggle", "enabled", DataValue::Boolean(true)))).unwrap();
        graph.add_node(Box::new(StaticOutputNode::new(
            "source",
            "value",
            DataValue::Json(serde_json::json!({
                "content": "hello",
                "is_at_me": false,
            })),
        ))).unwrap();
        graph.add_node(Box::new(SwitchNode::new("gate", "Gate"))).unwrap();
        graph.add_node(Box::new(SeenSinkNode::new("sink", "value", DataType::Json))).unwrap();
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
    fn root_event_dispatch_runs_same_sender_workers_concurrently() {
        ensure_test_dispatch_nodes_registered();
        reset_dispatch_stats();

        let definition: NodeGraphDefinition = serde_json::from_value(serde_json::json!({
            "nodes": [
                {
                    "id": "root",
                    "name": "Root",
                    "node_type": "__test_full_concurrent_event_producer",
                    "input_ports": [],
                    "output_ports": [
                        {
                            "name": "sender_id",
                            "data_type": "String",
                            "required": true
                        }
                    ],
                    "inline_values": {}
                },
                {
                    "id": "sink",
                    "name": "Sink",
                    "node_type": "__test_full_concurrent_sleep_sink",
                    "input_ports": [
                        {
                            "name": "sender_id",
                            "data_type": "String",
                            "required": true
                        }
                    ],
                    "output_ports": [
                        {
                            "name": "done",
                            "data_type": "Boolean",
                            "required": true
                        }
                    ],
                    "inline_values": {}
                }
            ],
            "edges": []
        }))
        .expect("test graph definition should deserialize");

        let mut graph = build_node_graph_from_definition(&definition)
            .expect("test graph should build");
        graph.execute().expect("test graph should execute");

        let stats = TEST_DISPATCH_STATS.lock().unwrap();
        assert_eq!(stats.seen_sender_ids.len(), 2);
        assert!(stats.seen_sender_ids.iter().all(|sender_id| sender_id == "same-sender"));
        assert!(
            stats.max_in_flight >= 2,
            "expected same-sender workers to overlap, got stats: {:?}",
            *stats
        );
    }

    #[test]
    fn qq_agent_example_graph_builds_with_session_lock_nodes() {
        init_node_registry().expect("built-in node registry should initialize");

        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("qq_agent_example.json");
        let definition = load_graph_definition_from_json(&path)
            .expect("example graph JSON should load");
        build_node_graph_from_definition(&definition)
            .expect("example graph should rebuild into a runnable NodeGraph");
    }
}
