//! Builds a callable tool out of one registered DAG node.
//!
//! A node is not a graph, so it cannot be handed to `NodeGraphTool` as-is. This module wraps a
//! node type into the function-shaped subgraph that tool runtime expects: boundary nodes for the
//! signature plus the node itself. Ports whose data type is a runtime resource reference
//! (`RdbRef`, `RetrievalStoreRef`, ...) are filled by provider nodes reading the agent context, so
//! the LLM only ever sees the plain-value parameters it can actually produce.

use crate::error::{Error, Result};
use crate::graph::function_graph::{
    default_function_subgraph, FunctionPortDef, FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
};
use crate::graph::graph_io::{EdgeDefinition, GraphPosition, NodeDefinition, NodeGraphDefinition};
use crate::graph::registry::NODE_REGISTRY;
use crate::graph::tool_spec::{ToolDefinition, ToolParamDef};
use crate::graph::{DataType, Port};

/// Node id the wrapped node receives inside the synthesized subgraph.
const NODE_TOOL_TARGET_ID: &str = "__node_tool_target__";

/// Config the `agent_llm` provider needs, since its model choice cannot come from the caller.
const DEFAULT_LLM_KIND_FIELD: &str = "llm_kind";
const DEFAULT_LLM_KIND: &str = "main";

/// A node type that reads one agent-runtime resource and outputs it as a reference.
struct ResourceProvider {
    node_type: &'static str,
    output_port: &'static str,
}

/// Maps a reference port type to the node that supplies it from the agent runtime.
///
/// Ports typed like this cannot be produced by an LLM, so a single-node tool wires the provider
/// in automatically instead of exposing the port as a parameter.
fn resource_provider_for(data_type: &DataType) -> Option<ResourceProvider> {
    let provider = match data_type {
        DataType::RdbRef => ResourceProvider {
            node_type: "agent_rdb_ref",
            output_port: "rdb_ref",
        },
        DataType::S3Ref => ResourceProvider {
            node_type: "agent_rustfs_ref",
            output_port: "s3_ref",
        },
        DataType::RetrievalStoreRef => ResourceProvider {
            node_type: "agent_retrieval_store_ref",
            output_port: "retrieval_store_ref",
        },
        DataType::EmbeddingModel => ResourceProvider {
            node_type: "agent_embedding_model",
            output_port: "embedding_model",
        },
        DataType::WebSearchEngineRef => ResourceProvider {
            node_type: "agent_tavily_ref",
            output_port: "web_search_engine_ref",
        },
        DataType::LLModel => ResourceProvider {
            node_type: "agent_llm",
            output_port: "llm_model",
        },
        _ => return None,
    };
    Some(provider)
}

/// Node types that can currently be wrapped into a tool.
///
/// Used to build the allow-list an agent definition validates its `tool_ids` against, so a
/// definition naming a node type is accepted exactly when the node can actually be called.
pub fn available_node_tool_ids() -> Vec<String> {
    NODE_REGISTRY
        .get_all_types()
        .into_iter()
        .filter(|metadata| {
            let Ok((_, output_ports)) = node_tool_ports(&metadata.type_id) else {
                return false;
            };
            check_node_is_tool_capable(&metadata.type_id, &output_ports).is_ok()
        })
        .map(|metadata| metadata.type_id)
        .collect()
}

/// The ports a registered node type declares.
fn node_tool_ports(node_type: &str) -> Result<(Vec<Port>, Vec<Port>)> {
    NODE_REGISTRY.get_node_ports(node_type).ok_or_else(|| {
        Error::ValidationError(format!("节点工具无法读取节点类型 '{node_type}' 的端口"))
    })
}

/// Rejects node types that cannot be expressed as a callable tool.
///
/// A dynamic input port means the port list only exists after inline configuration, and no output
/// port means the call could never return anything.
fn check_node_is_tool_capable(node_type: &str, output_ports: &[Port]) -> Result<()> {
    if let Some((dynamic_input, _)) = NODE_REGISTRY.get_node_dynamic_port_flags(node_type) {
        if dynamic_input {
            return Err(Error::ValidationError(format!(
                "节点 '{node_type}' 具有动态输入端口，无法作为工具调用"
            )));
        }
    }
    if output_ports.is_empty() {
        return Err(Error::ValidationError(format!(
            "节点 '{node_type}' 没有输出端口，无法作为工具调用"
        )));
    }
    Ok(())
}

/// Wraps one registered node type into a tool definition.
///
/// The node's plain-value input ports become the tool parameters; its reference inputs are
/// satisfied from the agent runtime and its outputs become the tool outputs.
pub fn build_node_tool_definition(node_type: &str) -> Result<ToolDefinition> {
    let node_type = node_type.trim();
    if node_type.is_empty() {
        return Err(Error::ValidationError("节点工具缺少节点类型".to_string()));
    }
    let metadata = NODE_REGISTRY.get_node_metadata(node_type).ok_or_else(|| {
        Error::ValidationError(format!("节点工具引用了未注册的节点类型 '{node_type}'"))
    })?;
    let (input_ports, output_ports) = node_tool_ports(node_type)?;
    check_node_is_tool_capable(node_type, &output_ports)?;

    let inputs = input_ports
        .iter()
        .map(|port| FunctionPortDef {
            name: port.name.clone(),
            data_type: port.data_type.clone(),
            description: port.description.clone().unwrap_or_default(),
            required: port.required,
        })
        .collect::<Vec<_>>();
    let outputs = output_ports
        .iter()
        .map(|port| FunctionPortDef {
            name: port.name.clone(),
            data_type: port.data_type.clone(),
            description: port.description.clone().unwrap_or_default(),
            required: true,
        })
        .collect::<Vec<_>>();

    let subgraph = synthesize_node_subgraph(node_type, &inputs, &outputs)?;
    Ok(ToolDefinition {
        id: node_type.to_string(),
        name: node_type.to_string(),
        description: metadata.description,
        run_duration: Default::default(),
        implementation: Default::default(),
        built_in_kind: None,
        script_config: None,
        parameters: parameters_from_inputs(&inputs),
        outputs,
        subgraph,
    })
}

/// Parameter list the LLM sees: every input port the runtime does not fill itself.
fn parameters_from_inputs(inputs: &[FunctionPortDef]) -> Vec<ToolParamDef> {
    inputs
        .iter()
        .filter(|port| resource_provider_for(&port.data_type).is_none())
        .map(|port| ToolParamDef {
            name: port.name.clone(),
            data_type: port.data_type.clone(),
            desc: port.description.clone(),
            required: port.required,
        })
        .collect()
}

/// Builds the function-shaped subgraph wrapping one node.
///
/// Each synthesized node carries the ports its type declares: the signature sync validates every
/// edge against them, so an edge whose endpoint node has no declared port would be dropped.
fn synthesize_node_subgraph(
    node_type: &str,
    inputs: &[FunctionPortDef],
    outputs: &[FunctionPortDef],
) -> Result<NodeGraphDefinition> {
    let (target_inputs, target_outputs) = registered_ports(node_type)?;
    let mut graph = default_function_subgraph();
    graph.nodes.push(NodeDefinition {
        id: NODE_TOOL_TARGET_ID.to_string(),
        name: node_type.to_string(),
        description: None,
        node_type: node_type.to_string(),
        input_ports: target_inputs,
        output_ports: target_outputs,
        output: None,
        execution_time: None,
        dynamic_input_ports: false,
        dynamic_output_ports: false,
        position: Some(GraphPosition { x: 900.0, y: 400.0 }),
        size: None,
        inline_values: Default::default(),
        ui_state: None,
        port_bindings: Default::default(),
        has_error: false,
        has_cycle: false,
        disabled: false,
    });

    // Plain-value ports are fed from the function inputs boundary.
    for port in inputs.iter().filter(|port| resource_provider_for(&port.data_type).is_none()) {
        graph.edges.push(EdgeDefinition {
            from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
            from_port: port.name.clone(),
            to_node_id: NODE_TOOL_TARGET_ID.to_string(),
            to_port: port.name.clone(),
        });
    }

    // Reference ports are fed by a provider node reading the agent runtime.
    let mut provider_index = 0usize;
    for port in inputs.iter() {
        let Some(provider) = resource_provider_for(&port.data_type) else {
            continue;
        };
        let provider_id = format!("__node_tool_resource_{provider_index}__");
        provider_index += 1;
        let (provider_inputs, provider_outputs) = registered_ports(provider.node_type)?;
        let mut inline_values = std::collections::HashMap::new();
        if provider.node_type == "agent_llm" {
            inline_values
                .insert(DEFAULT_LLM_KIND_FIELD.to_string(), serde_json::json!(DEFAULT_LLM_KIND));
        }
        graph.nodes.push(NodeDefinition {
            id: provider_id.clone(),
            name: provider.node_type.to_string(),
            description: None,
            node_type: provider.node_type.to_string(),
            input_ports: provider_inputs,
            output_ports: provider_outputs,
            output: None,
            execution_time: None,
            dynamic_input_ports: false,
            dynamic_output_ports: false,
            position: Some(GraphPosition {
                x: 150.0,
                y: 120.0 + provider_index as f32 * 160.0,
            }),
            size: None,
            inline_values,
            ui_state: None,
            port_bindings: Default::default(),
            has_error: false,
            has_cycle: false,
            disabled: false,
        });
        graph.edges.push(EdgeDefinition {
            from_node_id: provider_id,
            from_port: provider.output_port.to_string(),
            to_node_id: NODE_TOOL_TARGET_ID.to_string(),
            to_port: port.name.clone(),
        });
    }

    for port in outputs {
        graph.edges.push(EdgeDefinition {
            from_node_id: NODE_TOOL_TARGET_ID.to_string(),
            from_port: port.name.clone(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: port.name.clone(),
        });
    }

    crate::graph::function_graph::sync_function_subgraph_signature(&mut graph, inputs, outputs);
    if graph.edges.is_empty() {
        return Err(Error::ValidationError(format!(
            "节点 '{node_type}' 合成工具子图后没有保留任何连边，无法作为工具调用"
        )));
    }
    Ok(graph)
}

/// The ports a registered node type declares, for embedding into a synthesized definition.
fn registered_ports(node_type: &str) -> Result<(Vec<Port>, Vec<Port>)> {
    NODE_REGISTRY
        .get_node_ports(node_type)
        .ok_or_else(|| Error::ValidationError(format!("无法读取节点类型 '{node_type}' 的端口定义")))
}
