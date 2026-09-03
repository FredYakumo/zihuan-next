use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zihuan_core::graph::graph_io::NodeGraphDefinition;

pub struct GraphSession {
    pub id: String,
    /// Optional filesystem path for save/load
    pub file_path: Option<String>,
    pub graph: NodeGraphDefinition,
    pub dirty: bool,
}

impl GraphSession {
    pub fn new(id: String, graph: NodeGraphDefinition, file_path: Option<String>) -> Self {
        Self { id, file_path, graph, dirty: false }
    }

    pub fn new_empty() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            file_path: None,
            graph: zihuan_core::graph::graph_boundary::default_root_graph_definition(),
            dirty: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GraphTabInfo {
    pub id: String,
    pub name: String,
    pub file_path: Option<String>,
    pub dirty: bool,
    pub node_count: usize,
    pub edge_count: usize,
}
