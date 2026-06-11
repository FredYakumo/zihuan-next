use std::cell::RefCell;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

thread_local! {
    static CURRENT_WORKSPACE_CONTEXT: RefCell<Vec<WorkspaceContext>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskUserRequest {
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceContext {
    pub workspace_path: Option<PathBuf>,
}

pub fn with_workspace_context<T>(context: WorkspaceContext, f: impl FnOnce() -> T) -> T {
    CURRENT_WORKSPACE_CONTEXT.with(|slot| {
        slot.borrow_mut().push(context);
    });
    let result = f();
    CURRENT_WORKSPACE_CONTEXT.with(|slot| {
        slot.borrow_mut().pop();
    });
    result
}

pub fn current_workspace_context() -> Result<WorkspaceContext> {
    CURRENT_WORKSPACE_CONTEXT.with(|slot| {
        slot.borrow().last().cloned().ok_or_else(|| {
            Error::ValidationError("当前工具调用不在 workspace agent 上下文中".to_string())
        })
    })
}

pub fn resolve_workspace_path(path: &str) -> Result<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError("path must not be empty".to_string()));
    }

    let target = PathBuf::from(trimmed);
    if target.is_absolute() {
        return Ok(target);
    }

    let context = current_workspace_context()?;
    let base = context.workspace_path.ok_or_else(|| {
        Error::ValidationError("当前 workspace agent 会话尚未选择工作目录".to_string())
    })?;
    Ok(base.join(target))
}

pub fn workspace_cwd_or(current: Option<&str>) -> Result<PathBuf> {
    if let Some(value) = current.map(str::trim).filter(|value| !value.is_empty()) {
        return resolve_workspace_path(value);
    }

    let context = current_workspace_context()?;
    context.workspace_path.ok_or_else(|| {
        Error::ValidationError("当前 workspace agent 会话尚未选择工作目录".to_string())
    })
}

pub fn normalized_workspace_path(path: Option<&str>) -> Option<String> {
    path.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Path::new(value).to_string_lossy().to_string())
}
