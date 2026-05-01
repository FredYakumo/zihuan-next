use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};

// ─── Theme data structures ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct ThemeConfig {
    pub name: String,
    pub display_name: String,
    pub mode: String,
    pub css: serde_json::Value,
    pub litegraph: serde_json::Value,
}

#[derive(Serialize)]
pub struct ThemeListItem {
    pub name: String,
    pub display_name: String,
    pub mode: String,
}

#[derive(Serialize)]
pub struct ThemeListResponse {
    pub themes: Vec<ThemeListItem>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// List all theme JSON files in the `custom_themes/` directory.
#[handler]
pub async fn list_themes(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let dir = std::path::Path::new("custom_themes");
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            res.render(Json(serde_json::json!({ "themes": [] })));
            return;
        }
    };

    let mut themes: Vec<ThemeListItem> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                return None;
            }
            let content = std::fs::read_to_string(&p).ok()?;
            let config: ThemeConfig = serde_json::from_str(&content).ok()?;
            Some(ThemeListItem {
                name: config.name,
                display_name: config.display_name,
                mode: config.mode,
            })
        })
        .collect();

    themes.sort_by(|a, b| a.name.cmp(&b.name));
    res.render(Json(ThemeListResponse { themes }));
}

/// Get a single theme configuration by name.
#[handler]
pub async fn get_theme(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let name = req.param::<String>("name").unwrap_or_default();
    // Security: reject path traversal
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({ "error": "Invalid theme name" })));
        return;
    }

    let path = std::path::Path::new("custom_themes").join(format!("{}.json", name));
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            match serde_json::from_str::<ThemeConfig>(&content) {
                Ok(config) => {
                    res.render(Json(config));
                }
                Err(e) => {
                    res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                    res.render(Json(serde_json::json!({ "error": e.to_string() })));
                }
            }
        }
        Err(_) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({ "error": "Theme not found" })));
        }
    }
}
