//! Workspace Agent runtime and its HTTP-facing workspace capabilities.

pub mod api;
pub mod tools;
pub mod workspace_agent_service;
pub mod task_tracking { pub use crate::tools::workspace_tools::task_tracking::{delete_workspace_tasks, load_workspace_tasks, WorkspaceTask, WorkspaceTaskSnapshot, WorkspaceTaskStatus}; }

#[cfg(test)]
mod tests;
