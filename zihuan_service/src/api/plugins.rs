use salvo::prelude::*;
use serde::Serialize;

pub mod script_plugin;

#[derive(Serialize)]
pub struct PluginRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub plugin_type: String,
    pub display_name: String,
    pub description: String,
}

pub(crate) fn render_error(res: &mut Response, status: StatusCode, error: String) {
    res.status_code(status);
    res.render(Json(serde_json::json!({ "error": error })));
}

#[handler]
pub async fn list_plugins(res: &mut Response) {
    res.render(Json(script_plugin_records()));
}

fn script_plugin_records() -> Vec<PluginRecord> {
    zihuan_core::script_plugins::script_plugin_catalog()
        .into_iter()
        .map(|plugin| PluginRecord {
            id: format!("dynamic-script:{}", plugin.name),
            name: plugin.name.clone(),
            version: if plugin.version.trim().is_empty() {
                "0.0.0".to_string()
            } else {
                plugin.version.clone()
            },
            plugin_type: plugin.plugin_type,
            display_name: plugin.display_name,
            description: plugin.description,
        })
        .collect()
}
