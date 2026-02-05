use slint::{ModelRc, VecModel};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::error::Result;
use crate::node::graph_io::{
    ensure_positions,
    load_graph_definition_from_json,
    NodeGraphDefinition,
};
use crate::node::registry::NODE_REGISTRY;

// 引入分离的 UI 定义
use crate::ui::graph_window::*;

pub fn show_graph(initial_graph: Option<NodeGraphDefinition>) -> Result<()> {
    register_cjk_fonts();

    let ui = NodeGraphWindow::new()
        .map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))?;

    let graph_state = Rc::new(RefCell::new(initial_graph.unwrap_or_default()));
    let current_file = Rc::new(RefCell::new(
        if graph_state.borrow().nodes.is_empty() && graph_state.borrow().edges.is_empty() {
            "未加载 节点图".to_string()
        } else {
            "已加载 节点图".to_string()
        },
    ));

    // Load available node types from registry
    let node_types: Vec<NodeTypeVm> = NODE_REGISTRY
        .get_all_types()
        .iter()
        .map(|meta| NodeTypeVm {
            type_id: meta.type_id.clone().into(),
            display_name: meta.display_name.clone().into(),
            category: meta.category.clone().into(),
            description: meta.description.clone().into(),
        })
        .collect();
    
    ui.set_available_node_types(ModelRc::new(VecModel::from(node_types)));

    apply_graph_to_ui(
        &ui,
        &graph_state.borrow(),
        Some(current_file.borrow().clone()),
    );

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    ui.on_open_json(move || {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Node Graph", &["json"])
            .pick_file()
        {
            if let Ok(graph) = load_graph_definition_from_json(&path) {
                if let Some(ui) = ui_handle.upgrade() {
                    *graph_state_clone.borrow_mut() = graph;
                    let label = path.display().to_string();
                    *current_file_clone.borrow_mut() = label.clone();
                    apply_graph_to_ui(&ui, &graph_state_clone.borrow(), Some(label));
                }
            }
        }
    });

    let ui_handle = ui.as_weak();
    let graph_state_clone = Rc::clone(&graph_state);
    let current_file_clone = Rc::clone(&current_file);
    ui.on_add_node(move |type_id| {
        let type_id_str = type_id.as_str();
        let mut graph = graph_state_clone.borrow_mut();
        if let Err(e) = add_node_to_graph(&mut graph, type_id_str) {
            eprintln!("Failed to add node: {}", e);
            return;
        }
        let label = "已修改(未保存)".to_string();
        *current_file_clone.borrow_mut() = label.clone();
        if let Some(ui) = ui_handle.upgrade() {
            apply_graph_to_ui(&ui, &graph, Some(label));
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_show_node_type_menu(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_node_selector(true);
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_hide_node_type_menu(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_node_selector(false);
        }
    });

    ui.run()
        .map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))
}

fn register_cjk_fonts() {
    use slint::fontique_07::{fontique, shared_collection};
    use std::sync::Arc;

    let candidates = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/Library/Fonts/NotoSansCJKsc-Regular.otf",
        "/Library/Fonts/NotoSansCJK-Regular.ttc",
    ];

    let mut collection = shared_collection();

    for path in candidates {
        if !Path::new(path).exists() {
            continue;
        }

        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };

        let blob = fontique::Blob::new(Arc::new(bytes));
        let fonts = collection.register_fonts(blob, None);
        if fonts.is_empty() {
            continue;
        }

        let ids: Vec<_> = fonts.iter().map(|font| font.0).collect();
        let hani = fontique::FallbackKey::new("Hani", None);
        let hira = fontique::FallbackKey::new("Hira", None);
        let kana = fontique::FallbackKey::new("Kana", None);

        collection.append_fallbacks(hani, ids.iter().copied());
        collection.append_fallbacks(hira, ids.iter().copied());
        collection.append_fallbacks(kana, ids.iter().copied());
    }
}

fn apply_graph_to_ui(
    ui: &NodeGraphWindow,
    graph: &NodeGraphDefinition,
    current_file: Option<String>,
) {
    let mut graph = graph.clone();
    ensure_positions(&mut graph);

    let nodes: Vec<NodeVm> = graph
        .nodes
        .iter()
        .map(|node| {
            let position = node.position.as_ref();
            let label = format!("{} ({})", node.name, node.id);
            NodeVm {
                label: label.into(),
                x: position.map(|p| p.x).unwrap_or(0.0),
                y: position.map(|p| p.y).unwrap_or(0.0),
            }
        })
        .collect();

    let edges: Vec<EdgeVm> = graph
        .edges
        .iter()
        .map(|edge| EdgeVm {
            label: format!(
                "{}:{} → {}:{}",
                edge.from_node_id, edge.from_port, edge.to_node_id, edge.to_port
            )
            .into(),
        })
        .collect();

    let label = current_file.unwrap_or_else(|| "已加载 JSON".to_string());

    ui.set_nodes(ModelRc::new(VecModel::from(nodes)));
    ui.set_edges(ModelRc::new(VecModel::from(edges)));
    ui.set_current_file(label.into());
}

fn add_node_to_graph(graph: &mut NodeGraphDefinition, type_id: &str) -> Result<()> {
    let id = next_node_id(graph);
    
    // Get metadata from registry
    let all_types = NODE_REGISTRY.get_all_types();
    let metadata = all_types.iter().find(|meta| meta.type_id == type_id);
    
    let display_name = metadata
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| "NewNode".to_string());

    // Create a dummy node instance to get port information
    let dummy_node = NODE_REGISTRY.create_node(type_id, &id, &display_name)?;
    
    graph.nodes.push(crate::node::graph_io::NodeDefinition {
        id,
        name: display_name,
        description: dummy_node.description().map(|s| s.to_string()),
        node_type: type_id.to_string(),
        input_ports: dummy_node.input_ports(),
        output_ports: dummy_node.output_ports(),
        position: None,
    });
    
    Ok(())
}

fn next_node_id(graph: &NodeGraphDefinition) -> String {
    let mut index = 1usize;
    loop {
        let candidate = format!("node_{index}");
        if !graph.nodes.iter().any(|node| node.id == candidate) {
            return candidate;
        }
        index += 1;
    }
}
