//! AGENTS.md management for Workspace Agent Service.
//!
//! Lists, creates, edits, and deletes the `AGENTS.md` files that a Workspace Agent can
//! reference in its system prompt. Files are addressed by a location key rather than a raw
//! path, so every write is confined to one of the three candidate directories.

use std::fs;

use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::agent::workspace_agent_service::{
    agents_md_candidates, AgentsMdCandidate, AgentsMdLocation,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsMdFile {
    pub key: String,
    pub path: String,
    pub exists: bool,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveAgentsMdRequest {
    pub key: String,
    pub content: String,
}

/// Lists the candidate AGENTS.md files in priority order, including their current content.
#[handler]
pub async fn list_agents_md(req: &mut Request, res: &mut Response) {
    let workspace_path = query_workspace_path(req);
    let files = agents_md_candidates(workspace_path.as_deref())
        .into_iter()
        .map(|candidate| {
            let content = if candidate.exists {
                fs::read_to_string(&candidate.path).unwrap_or_default()
            } else {
                String::new()
            };
            AgentsMdFile {
                key: candidate.location.key().to_string(),
                path: candidate.path.to_string_lossy().to_string(),
                exists: candidate.exists,
                content,
            }
        })
        .collect::<Vec<_>>();
    res.render(Json(json!({ "files": files })));
}

/// Creates or overwrites the AGENTS.md file at the requested location.
#[handler]
pub async fn save_agents_md(req: &mut Request, res: &mut Response) {
    let body: SaveAgentsMdRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => {
            return render_error(res, StatusCode::BAD_REQUEST, format!("请求体解析失败: {err}"));
        }
    };
    let workspace_path = query_workspace_path(req);
    let candidate = match resolve_candidate(&body.key, workspace_path.as_deref()) {
        Some(candidate) => candidate,
        None => {
            return render_error(res, StatusCode::BAD_REQUEST, format!("无效的 AGENTS.md 位置: {}", body.key));
        }
    };
    if let Some(parent) = candidate.path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, format!("创建目录失败: {err}"));
        }
    }
    if let Err(err) = fs::write(&candidate.path, &body.content) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, format!("写入失败: {err}"));
    }
    res.render(Json(json!({ "ok": true, "path": candidate.path.to_string_lossy().to_string() })));
}

/// Deletes the AGENTS.md file at the requested location, if it exists.
#[handler]
pub async fn delete_agents_md(req: &mut Request, res: &mut Response) {
    let key = req.query::<String>("key").unwrap_or_default();
    let workspace_path = query_workspace_path(req);
    let candidate = match resolve_candidate(&key, workspace_path.as_deref()) {
        Some(candidate) => candidate,
        None => {
            return render_error(res, StatusCode::BAD_REQUEST, format!("无效的 AGENTS.md 位置: {key}"));
        }
    };
    if candidate.exists {
        if let Err(err) = fs::remove_file(&candidate.path) {
            return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, format!("删除失败: {err}"));
        }
    }
    res.render(Json(json!({ "ok": true, "path": candidate.path.to_string_lossy().to_string() })));
}

fn query_workspace_path(req: &Request) -> Option<String> {
    req.query::<String>("workspace_path")
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
}

fn resolve_candidate(key: &str, workspace_path: Option<&str>) -> Option<AgentsMdCandidate> {
    let location = match key {
        "workspace" => AgentsMdLocation::Workspace,
        "executable" => AgentsMdLocation::Executable,
        "home" => AgentsMdLocation::Home,
        _ => return None,
    };
    agents_md_candidates(workspace_path)
        .into_iter()
        .find(|candidate| candidate.location == location)
}

fn render_error(res: &mut Response, status: StatusCode, message: String) {
    res.status_code(status);
    res.render(Json(json!({ "error": message })));
}
