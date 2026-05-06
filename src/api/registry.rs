use salvo::prelude::*;
use salvo::writing::Json;
use serde::Serialize;

use zihuan_graph_engine::registry::NODE_REGISTRY;

#[derive(Serialize)]
pub struct PortInfo {
    pub name: String,
    pub data_type: String,
    pub description: Option<String>,
    pub required: bool,
    pub hidden: bool,
}

#[derive(Serialize)]
pub struct NodeConfigFieldInfo {
    pub key: String,
    pub data_type: String,
    pub description: Option<String>,
    pub required: bool,
    pub widget: String,
    pub connection_kind: Option<String>,
}

#[derive(Serialize)]
pub struct NodeTypeInfo {
    pub type_id: String,
    pub display_name: String,
    pub category: String,
    pub description: String,
    pub input_ports: Vec<PortInfo>,
    pub output_ports: Vec<PortInfo>,
    pub has_dynamic_input_ports: bool,
    pub has_dynamic_output_ports: bool,
    pub is_event_producer: bool,
    pub config_fields: Vec<NodeConfigFieldInfo>,
}

#[derive(Serialize)]
pub struct RegistryResponse {
    pub types: Vec<NodeTypeInfo>,
    pub categories: Vec<String>,
}

#[handler]
pub async fn get_registry(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let all_types = NODE_REGISTRY.get_all_types();

    let mut types: Vec<NodeTypeInfo> = all_types
        .iter()
        .map(|meta| {
            let (input_ports, output_ports) = NODE_REGISTRY
                .get_node_ports(&meta.type_id)
                .unwrap_or_default();

            let (has_dyn_in, has_dyn_out) = NODE_REGISTRY
                .get_node_dynamic_port_flags(&meta.type_id)
                .unwrap_or((false, false));
            let config_fields = NODE_REGISTRY
                .get_node_config_fields(&meta.type_id)
                .unwrap_or_default();

            NodeTypeInfo {
                type_id: meta.type_id.clone(),
                display_name: meta.display_name.clone(),
                category: meta.category.clone(),
                description: meta.description.clone(),
                input_ports: input_ports
                    .iter()
                    .map(|p| PortInfo {
                        name: p.name.clone(),
                        data_type: format!("{:?}", p.data_type),
                        description: p.description.clone(),
                        required: p.required,
                        hidden: p.hidden,
                    })
                    .collect(),
                output_ports: output_ports
                    .iter()
                    .map(|p| PortInfo {
                        name: p.name.clone(),
                        data_type: format!("{:?}", p.data_type),
                        description: p.description.clone(),
                        required: p.required,
                        hidden: p.hidden,
                    })
                    .collect(),
                has_dynamic_input_ports: has_dyn_in,
                has_dynamic_output_ports: has_dyn_out,
                is_event_producer: false,
                config_fields: config_fields
                    .iter()
                    .map(|field| NodeConfigFieldInfo {
                        key: field.key.clone(),
                        data_type: format!("{:?}", field.data_type),
                        description: field.description.clone(),
                        required: field.required,
                        widget: serde_json::to_value(&field.widget)
                            .ok()
                            .and_then(|value| value.as_str().map(str::to_string))
                            .unwrap_or_else(|| "connection_select".to_string()),
                        connection_kind: field.connection_kind.clone(),
                    })
                    .collect(),
            }
        })
        .collect();

    // Stable sort by category then display_name
    types.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });

    let categories: Vec<String> = {
        let mut cats: Vec<String> = types.iter().map(|t| t.category.clone()).collect();
        cats.sort();
        cats.dedup();
        cats
    };

    res.render(Json(RegistryResponse { types, categories }));
}

#[handler]
pub async fn get_categories(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let cats = NODE_REGISTRY.get_categories();
    res.render(Json(cats));
}

