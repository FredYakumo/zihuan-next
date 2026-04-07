use std::sync::Arc;

use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zihuan_node::graph_io::{GraphMetadata, GraphPosition, GraphSize, NodeDefinition, NodeGraphDefinition, PortBinding};

use super::state::{AppState, GraphSession, GraphTabInfo};

// ─── List open graphs ─────────────────────────────────────────────────────────

#[handler]
pub async fn list_graphs(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let sessions = state.sessions.read().unwrap();
    let tabs: Vec<GraphTabInfo> = sessions
        .values()
        .map(|s| GraphTabInfo {
            id: s.id.clone(),
            name: session_display_name(s),
            file_path: s.file_path.clone(),
            dirty: s.dirty,
            node_count: s.graph.nodes.len(),
            edge_count: s.graph.edges.len(),
        })
        .collect();
    res.render(Json(tabs));
}

// ─── Create new empty graph ───────────────────────────────────────────────────

#[handler]
pub async fn create_graph(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let session = GraphSession::new_empty();
    let tab = GraphTabInfo {
        id: session.id.clone(),
        name: "Untitled".to_string(),
        file_path: None,
        dirty: false,
        node_count: 0,
        edge_count: 0,
    };
    state
        .sessions
        .write()
        .unwrap()
        .insert(session.id.clone(), session);
    res.render(Json(tab));
}

// ─── Get full graph ───────────────────────────────────────────────────────────

#[handler]
pub async fn get_graph(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let sessions = state.sessions.read().unwrap();
    match sessions.get(&id) {
        Some(s) => res.render(Json(&s.graph)),
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}

// ─── Replace full graph ───────────────────────────────────────────────────────

#[handler]
pub async fn put_graph(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let body: NodeGraphDefinition = match req.parse_json_with_max_size(usize::MAX).await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };
    let mut sessions = state.sessions.write().unwrap();
    match sessions.get_mut(&id) {
        Some(s) => {
            s.graph = body;
            s.dirty = true;
            res.render(Json(serde_json::json!({"ok": true})));
        }
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}

// ─── Delete (close) graph ─────────────────────────────────────────────────────

#[handler]
pub async fn delete_graph(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let removed = state.sessions.write().unwrap().remove(&id).is_some();
    if removed {
        res.render(Json(serde_json::json!({"ok": true})));
    } else {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "Graph not found"})));
    }
}

// ─── Add node ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddNodeRequest {
    pub node_type: String,
    pub name: Option<String>,
    pub x: f32,
    pub y: f32,
}

#[handler]
pub async fn add_node(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let body: AddNodeRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    // Probe ports from registry
    let (input_ports, output_ports) = zihuan_node::registry::NODE_REGISTRY
        .get_node_ports(&body.node_type)
        .unwrap_or_default();

    let (dyn_in, dyn_out) = zihuan_node::registry::NODE_REGISTRY
        .get_node_dynamic_port_flags(&body.node_type)
        .unwrap_or((false, false));

    let node_id = Uuid::new_v4().to_string();
    let display_name = body
        .name
        .unwrap_or_else(|| body.node_type.replace('_', " "));

    let node_def = NodeDefinition {
        id: node_id.clone(),
        name: display_name,
        description: None,
        node_type: body.node_type,
        input_ports,
        output_ports,
        dynamic_input_ports: dyn_in,
        dynamic_output_ports: dyn_out,
        position: Some(GraphPosition { x: body.x, y: body.y }),
        size: Some(GraphSize { width: 200.0, height: 120.0 }),
        inline_values: Default::default(),
        port_bindings: Default::default(),
        has_error: false,
        has_cycle: false,
    };

    let mut sessions = state.sessions.write().unwrap();
    match sessions.get_mut(&id) {
        Some(s) => {
            s.graph.nodes.push(node_def);
            s.dirty = true;
            res.render(Json(serde_json::json!({"id": node_id})));
        }
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}

// ─── Update node ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateNodeRequest {
    pub name: Option<String>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub inline_values: Option<serde_json::Value>,
    pub port_bindings: Option<serde_json::Value>,
}

#[handler]
pub async fn update_node(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let graph_id = req.param::<String>("id").unwrap_or_default();
    let node_id = req.param::<String>("node_id").unwrap_or_default();
    let body: UpdateNodeRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    let mut sessions = state.sessions.write().unwrap();
    let session = match sessions.get_mut(&graph_id) {
        Some(s) => s,
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
            return;
        }
    };

    let node = match session.graph.nodes.iter_mut().find(|n| n.id == node_id) {
        Some(n) => n,
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Node not found"})));
            return;
        }
    };

    if let Some(name) = body.name {
        node.name = name;
    }
    if let Some(x) = body.x {
        if let Some(pos) = &mut node.position {
            pos.x = x;
        } else {
            node.position = Some(GraphPosition { x, y: 0.0 });
        }
    }
    if let Some(y) = body.y {
        if let Some(pos) = &mut node.position {
            pos.y = y;
        } else {
            node.position = Some(GraphPosition { x: 0.0, y });
        }
    }
    if let Some(w) = body.width {
        if let Some(sz) = &mut node.size {
            sz.width = w;
        } else {
            node.size = Some(GraphSize { width: w, height: 100.0 });
        }
    }
    if let Some(h) = body.height {
        if let Some(sz) = &mut node.size {
            sz.height = h;
        } else {
            node.size = Some(GraphSize { width: 200.0, height: h });
        }
    }
    if let Some(iv) = body.inline_values {
        if let serde_json::Value::Object(map) = iv {
            for (k, v) in map {
                node.inline_values.insert(k, v);
            }
        }
    }
    if let Some(pb) = body.port_bindings {
        if let Ok(bindings) =
            serde_json::from_value::<std::collections::HashMap<String, PortBinding>>(pb)
        {
            for (k, v) in bindings {
                node.port_bindings.insert(k, v);
            }
        }
    }

    session.dirty = true;
    res.render(Json(serde_json::json!({"ok": true})));
}

// ─── Delete node ──────────────────────────────────────────────────────────────

#[handler]
pub async fn delete_node(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let graph_id = req.param::<String>("id").unwrap_or_default();
    let node_id = req.param::<String>("node_id").unwrap_or_default();

    let mut sessions = state.sessions.write().unwrap();
    let session = match sessions.get_mut(&graph_id) {
        Some(s) => s,
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
            return;
        }
    };

    let before = session.graph.nodes.len();
    session.graph.nodes.retain(|n| n.id != node_id);
    // Also remove all edges connected to this node
    session
        .graph
        .edges
        .retain(|e| e.from_node_id != node_id && e.to_node_id != node_id);

    if session.graph.nodes.len() < before {
        session.dirty = true;
        res.render(Json(serde_json::json!({"ok": true})));
    } else {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "Node not found"})));
    }
}

// ─── Add edge ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddEdgeRequest {
    pub source_node: String,
    pub source_port: String,
    pub target_node: String,
    pub target_port: String,
}

#[handler]
pub async fn add_edge(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let body: AddEdgeRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    let mut sessions = state.sessions.write().unwrap();
    let session = match sessions.get_mut(&id) {
        Some(s) => s,
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
            return;
        }
    };

    // Prevent duplicate edges to same target port
    session.graph.edges.retain(|e| {
        !(e.to_node_id == body.target_node && e.to_port == body.target_port)
    });

    session
        .graph
        .edges
        .push(zihuan_node::graph_io::EdgeDefinition {
            from_node_id: body.source_node,
            from_port: body.source_port,
            to_node_id: body.target_node,
            to_port: body.target_port,
        });
    session.dirty = true;
    res.render(Json(serde_json::json!({"ok": true})));
}

// ─── Delete edge ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DeleteEdgeRequest {
    pub source_node: String,
    pub source_port: String,
    pub target_node: String,
    pub target_port: String,
}

#[handler]
pub async fn delete_edge(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let body: DeleteEdgeRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    let mut sessions = state.sessions.write().unwrap();
    let session = match sessions.get_mut(&id) {
        Some(s) => s,
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
            return;
        }
    };

    let before = session.graph.edges.len();
    session.graph.edges.retain(|e| {
        !(e.from_node_id == body.source_node
            && e.from_port == body.source_port
            && e.to_node_id == body.target_node
            && e.to_port == body.target_port)
    });

    if session.graph.edges.len() < before {
        session.dirty = true;
        res.render(Json(serde_json::json!({"ok": true})));
    } else {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "Edge not found"})));
    }
}

// ─── Validate graph ───────────────────────────────────────────────────────────

#[handler]
pub async fn validate_graph(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let sessions = state.sessions.read().unwrap();
    let session = match sessions.get(&id) {
        Some(s) => s,
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
            return;
        }
    };

    let issues = zihuan_node::graph_io::validate_graph_definition(&session.graph);
    let cycle_nodes = zihuan_node::graph_io::find_cycle_node_ids(&session.graph);

    let has_errors = issues.iter().any(|i| i.severity == "error") || !cycle_nodes.is_empty();
    let issues_json: Vec<serde_json::Value> = issues
        .iter()
        .map(|i| serde_json::json!({"severity": i.severity, "message": i.message}))
        .collect();
    let cycle_vec: Vec<&String> = cycle_nodes.iter().collect();
    res.render(Json(serde_json::json!({
        "issues": issues_json,
        "cycle_nodes": cycle_vec,
        "has_errors": has_errors,
    })));
}

// ─── Get / update graph metadata ─────────────────────────────────────────────

#[handler]
pub async fn get_metadata(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let sessions = state.sessions.read().unwrap();
    match sessions.get(&id) {
        Some(s) => res.render(Json(&s.graph.metadata)),
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateMetadataRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}

#[handler]
pub async fn update_metadata(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let body: UpdateMetadataRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };
    let mut sessions = state.sessions.write().unwrap();
    match sessions.get_mut(&id) {
        Some(s) => {
            s.graph.metadata = GraphMetadata {
                name: body.name,
                description: body.description,
                version: body.version,
            };
            s.dirty = true;
            res.render(Json(serde_json::json!({"ok": true})));
        }
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn session_display_name(s: &GraphSession) -> String {
    if let Some(path) = &s.file_path {
        std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    } else {
        "Untitled".to_string()
    }
}
