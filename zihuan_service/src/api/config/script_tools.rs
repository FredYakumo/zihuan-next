//! REST API handlers for script tool authoring.
//!
//! A script tool keeps its parameters and outputs in the agent configuration, but a script can
//! also declare them itself. This endpoint reads that declaration so the editor can fill the form
//! in from the code, the same way a node-graph tool is filled in from its graph.

use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use zihuan_core::graph::script_tool::materialize_script_source;
use zihuan_core::graph::tool_spec::{ScriptToolConfig, ScriptToolLanguage};

use super::{render_bad_request, render_unprocessable_entity};

/// Namespace the manifest scratch file is materialized under. Distinct from any tool id, because
/// a manifest read is a stateless inspection of a source that may not be saved yet.
const MANIFEST_INSPECTION_NAMESPACE: &str = "__tool_manifest";

#[derive(Deserialize)]
pub struct ScriptToolManifestRequest {
    pub language: ScriptToolLanguage,
    pub source: String,
    #[serde(default = "default_entry")]
    pub entry: String,
}

fn default_entry() -> String {
    "run_tool".to_string()
}

/// Reads the parameters and outputs one script declares, without running its tool entry.
///
/// The source is written under the same directory execution uses, so what the editor reads is
/// exactly what the engine would load.
#[handler]
pub async fn read_script_tool_manifest(req: &mut Request, res: &mut Response) {
    let body: ScriptToolManifestRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_bad_request(res, error.to_string()),
    };
    if body.source.trim().is_empty() {
        return render_bad_request(res, "脚本内容不能为空".to_string());
    }
    let config = ScriptToolConfig {
        language: body.language,
        source: body.source,
        entry: body.entry,
        timeout_secs: 60,
    };
    let workspace = match std::env::current_dir() {
        Ok(workspace) => workspace,
        Err(error) => return render_unprocessable_entity(res, error.to_string()),
    };
    let path = match materialize_script_source(MANIFEST_INSPECTION_NAMESPACE, &config) {
        Ok(path) => path,
        Err(error) => return render_unprocessable_entity(res, error.to_string()),
    };
    let runtime = match zihuan_core::config::ConfigCenter::shared().load_root() {
        Ok(runtime) => runtime,
        Err(error) => return render_unprocessable_entity(res, error.to_string()),
    };
    let result = dynamic_script_engine::load_tool_manifest(
        &workspace,
        config.language.engine_language(),
        &runtime.node_runtime,
        &runtime.python_runtime,
        &path,
    );
    match result {
        Ok(manifest) => res.render(Json(manifest)),
        Err(error) => render_unprocessable_entity(res, error.to_string()),
    }
}
