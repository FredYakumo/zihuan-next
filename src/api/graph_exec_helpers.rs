use ims_bot_adapter::active_adapter_manager::ActiveAdapterManager;
use serde_json::Value;
use std::collections::HashMap;
use storage_handler::{
    build_mysql_ref, build_redis_ref, build_s3_ref, build_tavily_ref, build_weaviate_ref,
    load_connections, ConnectionConfig,
};
use zihuan_core::error::Result;
use zihuan_graph_engine::data_value::DataType;
use zihuan_graph_engine::function_graph::{
    embedded_function_config_from_node, FUNCTION_CONFIG_PORT,
};
use zihuan_graph_engine::graph_io::{NodeGraphDefinition, PortBindingKind};
use zihuan_graph_engine::{DataValue, NodeGraph};

use crate::util::hyperparam_store;

pub fn apply_hyperparameter_bindings(
    graph: &mut NodeGraphDefinition,
    values: &HashMap<String, Value>,
) {
    for node in &mut graph.nodes {
        for (port_name, binding) in &node.port_bindings {
            if binding.kind != PortBindingKind::Hyperparameter {
                continue;
            }
            if let Some(value) = values.get(binding.name.as_str()) {
                node.inline_values.insert(port_name.clone(), value.clone());
            }
        }

        if let Some(mut config) = embedded_function_config_from_node(node) {
            apply_hyperparameter_bindings(&mut config.subgraph, values);
            if let Ok(value) = serde_json::to_value(&config) {
                node.inline_values
                    .insert(FUNCTION_CONFIG_PORT.to_string(), value);
            }
        }

        if let Some(tools_value) = node.inline_values.get("tools_config").cloned() {
            if let Ok(mut tools) = serde_json::from_value::<
                Vec<zihuan_core_nodes::brain::brain_tool::BrainToolDefinition>,
            >(tools_value)
            {
                for tool in &mut tools {
                    apply_hyperparameter_bindings(&mut tool.subgraph, values);
                }
                if let Ok(value) = serde_json::to_value(&tools) {
                    node.inline_values.insert("tools_config".to_string(), value);
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct RuntimeInlineValue {
    pub node_id: String,
    pub port_name: String,
    pub value: DataValue,
}

pub struct PreparedExecutionContext {
    pub definition: NodeGraphDefinition,
    pub runtime_inline_values: Vec<RuntimeInlineValue>,
    pub background_tasks: Vec<tokio::task::JoinHandle<()>>,
}

pub async fn prepare_execution_context(
    mut definition: NodeGraphDefinition,
    file_path: Option<&std::path::Path>,
) -> Result<PreparedExecutionContext> {
    if let Some(path) = file_path {
        let values = hyperparam_store::load_hyperparameter_values(path, &definition);
        apply_hyperparameter_bindings(&mut definition, &values);
    }

    let connections = load_connections()?;
    let mut runtime_inline_values = Vec::new();
    let mut background_tasks = Vec::new();

    for node in &definition.nodes {
        let port_types: HashMap<String, DataType> = node
            .input_ports
            .iter()
            .chain(node.output_ports.iter())
            .map(|port| (port.name.clone(), port.data_type.clone()))
            .collect();

        for (port_name, inline_value) in &node.inline_values {
            let Some(data_type) = port_types.get(port_name) else {
                continue;
            };
            let Some(connection_id) = inline_value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            let Some((value, background_task)) =
                resolve_connection_hyperparameter(data_type, connection_id, &connections).await?
            else {
                continue;
            };

            if let Some(task) = background_task {
                background_tasks.push(task);
            }

            runtime_inline_values.push(RuntimeInlineValue {
                node_id: node.id.clone(),
                port_name: port_name.clone(),
                value,
            });
        }
    }

    Ok(PreparedExecutionContext {
        definition,
        runtime_inline_values,
        background_tasks,
    })
}

pub fn inject_runtime_inline_values(
    graph: &mut NodeGraph,
    runtime_inline_values: &[RuntimeInlineValue],
) {
    for item in runtime_inline_values {
        graph
            .inline_values
            .entry(item.node_id.clone())
            .or_default()
            .insert(item.port_name.clone(), item.value.clone());
    }
}

async fn resolve_connection_hyperparameter(
    data_type: &DataType,
    connection_id: &str,
    connections: &[ConnectionConfig],
) -> Result<Option<(DataValue, Option<tokio::task::JoinHandle<()>>)>> {
    match data_type {
        DataType::MySqlRef => build_mysql_ref(Some(connection_id), connections)
            .await
            .map(|value| value.map(|value| (DataValue::MySqlRef(value), None))),
        DataType::RedisRef => build_redis_ref(Some(connection_id), connections)
            .map(|value| value.map(|value| (DataValue::RedisRef(value), None))),
        DataType::WeaviateRef => tokio::task::block_in_place(|| {
            build_weaviate_ref(Some(connection_id), connections, false)
        })
        .map(|value| value.map(|value| (DataValue::WeaviateRef(value), None))),
        DataType::S3Ref => build_s3_ref(Some(connection_id), connections)
            .await
            .map(|value| value.map(|value| (DataValue::S3Ref(value), None))),
        DataType::BotAdapterRef => build_ims_bot_adapter_ref(connection_id)
            .await
            .map(|value| value.map(|value| (value, None))),
        DataType::TavilyRef => build_tavily_ref(Some(connection_id), connections)
            .map(|value| value.map(|value| (DataValue::TavilyRef(value), None))),
        _ => Ok(None),
    }
}

async fn build_ims_bot_adapter_ref(connection_id: &str) -> Result<Option<DataValue>> {
    let handle = ActiveAdapterManager::shared()
        .get_active_bot_adapter_handle(connection_id)
        .await?;
    Ok(Some(DataValue::BotAdapterRef(handle)))
}
