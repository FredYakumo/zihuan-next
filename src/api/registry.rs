use salvo::prelude::*;
use salvo::writing::Json;
use serde::Serialize;
use std::sync::Arc;

use zihuan_node::registry::NODE_REGISTRY;

use super::state::AppState;

#[derive(Serialize)]
pub struct PortInfo {
    pub name: String,
    pub data_type: String,
    pub description: Option<String>,
    pub required: bool,
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

            let is_ep = NODE_REGISTRY.is_event_producer(&meta.type_id);

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
                    })
                    .collect(),
                output_ports: output_ports
                    .iter()
                    .map(|p| PortInfo {
                        name: p.name.clone(),
                        data_type: format!("{:?}", p.data_type),
                        description: p.description.clone(),
                        required: p.required,
                    })
                    .collect(),
                has_dynamic_input_ports: has_dyn_in,
                has_dynamic_output_ports: has_dyn_out,
                is_event_producer: is_ep,
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
