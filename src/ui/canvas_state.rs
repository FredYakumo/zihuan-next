use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CanvasViewState {
    pub pan_x: f32,
    pub pan_y: f32,
    pub zoom: f32,
}

impl Default for CanvasViewState {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }
}

pub fn load_canvas_view_state(graph_path: &Path) -> Option<CanvasViewState> {
    let path = canvas_state_path(graph_path)?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_canvas_view_state(graph_path: &Path, state: &CanvasViewState) -> std::io::Result<()> {
    let path = match canvas_state_path(graph_path) {
        Some(path) => path,
        None => return Ok(()),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(state)
        .unwrap_or_else(|_| "{\"pan_x\":0.0,\"pan_y\":0.0,\"zoom\":1.0}".to_string());
    fs::write(path, json)
}

fn canvas_state_path(graph_path: &Path) -> Option<PathBuf> {
    let base_dir = app_data_dir()?;
    let stem = graph_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "graph".to_string());
    let canonical_str = graph_path
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| graph_path.to_string_lossy().to_string());
    let hash = simple_hash8(&canonical_str);

    Some(
        base_dir
            .join("zihuan-next_aibot")
            .join("graph_views")
            .join(format!("{}_{}.json", stem, hash)),
    )
}

fn app_data_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .or_else(|_| std::env::var("LOCALAPPDATA"))
            .ok()
            .map(PathBuf::from)
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|home| PathBuf::from(home).join(".config"))
            })
    }
}

fn simple_hash8(s: &str) -> String {
    let mut h: u32 = 0x811c9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    format!("{:08x}", h)
}
