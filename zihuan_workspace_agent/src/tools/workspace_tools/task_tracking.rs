use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use zihuan_core::agent::brain::{BrainTool, ToolExecutionResource};
use zihuan_core::llm::tooling::{FunctionTool, StaticFunctionToolSpec};

use super::shared::json_error;

pub(crate) const DEFAULT_TOOL_TASK_CREATE: &str = "TaskCreate";
pub(crate) const DEFAULT_TOOL_TASK_UPDATE: &str = "TaskUpdate";
pub(crate) const DEFAULT_TOOL_TASK_GET: &str = "TaskGet";
pub(crate) const DEFAULT_TOOL_TASK_LIST: &str = "TaskList";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceTaskStatus { Pending, InProgress, Completed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTask {
    pub task_id: String,
    pub subject: String,
    #[serde(default)] pub description: String,
    pub active_form: String,
    #[serde(default)] pub metadata: Value,
    pub status: WorkspaceTaskStatus,
    #[serde(default)] pub blocks: Vec<String>,
    #[serde(default)] pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceTaskSnapshot { #[serde(default)] pub tasks: Vec<WorkspaceTask> }

pub fn load_workspace_tasks(session_id: &str) -> Result<WorkspaceTaskSnapshot, String> {
    let path = task_file_path(session_id)?;
    if !path.exists() { return Ok(WorkspaceTaskSnapshot::default()); }
    let file = OpenOptions::new().read(true).open(&path).map_err(|err| format!("failed to open task snapshot: {err}"))?;
    serde_json::from_reader(file).map_err(|err| format!("failed to parse task snapshot: {err}"))
}

pub fn delete_workspace_tasks(session_id: &str) -> Result<(), String> {
    let path = task_file_path(session_id)?;
    if path.exists() { fs::remove_file(path).map_err(|err| format!("failed to delete task snapshot: {err}"))?; }
    Ok(())
}

fn task_file_path(session_id: &str) -> Result<std::path::PathBuf, String> {
    if session_id.trim().is_empty() { return Err("Workspace task tools require a chat session".to_string()); }
    Ok(zihuan_core::system_config::app_data_dir().join("zihuan-next_aibot").join("chat_history").join(format!("{session_id}.tasks.json")))
}

fn save_workspace_tasks(session_id: &str, snapshot: &WorkspaceTaskSnapshot) -> Result<(), String> {
    let path = task_file_path(session_id)?;
    let parent = path.parent().ok_or_else(|| "task snapshot has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|err| format!("failed to create task snapshot directory: {err}"))?;
    let temporary = path.with_extension("tasks.json.tmp");
    let file = OpenOptions::new().create(true).truncate(true).write(true).open(&temporary).map_err(|err| format!("failed to write task snapshot: {err}"))?;
    serde_json::to_writer(file, snapshot).map_err(|err| format!("failed to serialize task snapshot: {err}"))?;
    fs::rename(temporary, path).map_err(|err| format!("failed to replace task snapshot: {err}"))
}

fn validate(snapshot: &WorkspaceTaskSnapshot) -> Result<(), String> {
    let ids: HashSet<&str> = snapshot.tasks.iter().map(|task| task.task_id.as_str()).collect();
    let mut active = 0usize;
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for task in &snapshot.tasks {
        if task.subject.trim().is_empty() || task.active_form.trim().is_empty() { return Err("task subject and activeForm must not be empty".to_string()); }
        if task.status == WorkspaceTaskStatus::InProgress { active += 1; }
        for dependency in task.blocked_by.iter().chain(task.blocks.iter()) {
            if !ids.contains(dependency.as_str()) { return Err(format!("task '{}' references unknown task '{dependency}'", task.task_id)); }
        }
        edges.insert(task.task_id.as_str(), task.blocked_by.iter().map(String::as_str).collect());
    }
    if active > 1 { return Err("only one task may be in_progress".to_string()); }
    fn visit<'a>(id: &'a str, edges: &HashMap<&'a str, Vec<&'a str>>, visiting: &mut HashSet<&'a str>, visited: &mut HashSet<&'a str>) -> bool {
        if !visiting.insert(id) { return true; }
        for next in edges.get(id).into_iter().flatten() { if visit(next, edges, visiting, visited) { return true; } }
        visiting.remove(id); visited.insert(id); false
    }
    let mut visiting = HashSet::new(); let mut visited = HashSet::new();
    if ids.iter().any(|id| !visited.contains(id) && visit(id, &edges, &mut visiting, &mut visited)) { return Err("task dependencies must not contain a cycle".to_string()); }
    for task in &snapshot.tasks {
        if task.status == WorkspaceTaskStatus::InProgress && task.blocked_by.iter().any(|id| snapshot.tasks.iter().any(|other| &other.task_id == id && other.status != WorkspaceTaskStatus::Completed)) {
            return Err(format!("task '{}' is blocked by an unfinished dependency", task.task_id));
        }
    }
    Ok(())
}

fn response(snapshot: &WorkspaceTaskSnapshot, task: Option<&WorkspaceTask>) -> String {
    serde_json::json!({ "ok": true, "task": task, "tasks": snapshot.tasks }).to_string()
}

#[derive(Clone)] pub(crate) struct WorkspaceTaskBrainTool { session_id: String, name: &'static str }
impl WorkspaceTaskBrainTool { pub(crate) fn new(session_id: String, name: &'static str) -> Self { Self { session_id, name } } }

impl BrainTool for WorkspaceTaskBrainTool {
    fn spec(&self) -> std::sync::Arc<dyn FunctionTool> {
        let parameters = match self.name {
            DEFAULT_TOOL_TASK_CREATE => serde_json::json!({"type":"object","properties":{"subject":{"type":"string"},"description":{"type":"string"},"activeForm":{"type":"string"},"metadata":{"type":"object"},"blocks":{"type":"array","items":{"type":"string"}},"blockedBy":{"type":"array","items":{"type":"string"}}},"required":["subject","activeForm"]}),
            DEFAULT_TOOL_TASK_UPDATE => serde_json::json!({"type":"object","properties":{"taskId":{"type":"string"},"subject":{"type":"string"},"description":{"type":"string"},"activeForm":{"type":"string"},"metadata":{"type":"object"},"status":{"enum":["pending","in_progress","completed","deleted"]},"blocks":{"type":"array","items":{"type":"string"}},"blockedBy":{"type":"array","items":{"type":"string"}}},"required":["taskId"]}),
            DEFAULT_TOOL_TASK_GET => serde_json::json!({"type":"object","properties":{"taskId":{"type":"string"}},"required":["taskId"]}),
            _ => serde_json::json!({"type":"object","properties":{}}),
        };
        std::sync::Arc::new(StaticFunctionToolSpec { name: self.name, description: "Track the current Workspace Chat task plan. Create a task list before non-trivial work; update one task at a time as work progresses.", parameters })
    }
    fn execute(&self, _: &str, arguments: &Value) -> String {
        let mut snapshot = match load_workspace_tasks(&self.session_id) { Ok(value) => value, Err(err) => return json_error(err) };
        match self.name {
            DEFAULT_TOOL_TASK_LIST => return response(&snapshot, None),
            DEFAULT_TOOL_TASK_GET => {
                let Some(id) = arguments.get("taskId").and_then(Value::as_str) else { return json_error("taskId is required"); };
                return match snapshot.tasks.iter().find(|task| task.task_id == id) { Some(task) => response(&snapshot, Some(task)), None => json_error(format!("task '{id}' was not found")) };
            }
            DEFAULT_TOOL_TASK_CREATE => {
                let Some(subject) = arguments.get("subject").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()) else { return json_error("subject is required"); };
                let Some(active_form) = arguments.get("activeForm").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()) else { return json_error("activeForm is required"); };
                snapshot.tasks.push(WorkspaceTask { task_id: format!("task_{}", Uuid::new_v4().simple()), subject: subject.to_string(), description: arguments.get("description").and_then(Value::as_str).unwrap_or_default().trim().to_string(), active_form: active_form.to_string(), metadata: arguments.get("metadata").cloned().unwrap_or_else(|| serde_json::json!({})), status: WorkspaceTaskStatus::Pending, blocks: string_list(arguments, "blocks"), blocked_by: string_list(arguments, "blockedBy") });
            }
            DEFAULT_TOOL_TASK_UPDATE => {
                let Some(id) = arguments.get("taskId").and_then(Value::as_str) else { return json_error("taskId is required"); };
                let Some(index) = snapshot.tasks.iter().position(|task| task.task_id == id) else { return json_error(format!("task '{id}' was not found")); };
                if arguments.get("status").and_then(Value::as_str) == Some("deleted") { snapshot.tasks.remove(index); } else {
                    let task = &mut snapshot.tasks[index];
                    if let Some(value) = arguments.get("subject").and_then(Value::as_str) { task.subject = value.trim().to_string(); }
                    if let Some(value) = arguments.get("description").and_then(Value::as_str) { task.description = value.trim().to_string(); }
                    if let Some(value) = arguments.get("activeForm").and_then(Value::as_str) { task.active_form = value.trim().to_string(); }
                    if let Some(value) = arguments.get("metadata") { task.metadata = value.clone(); }
                    if arguments.get("blocks").is_some() { task.blocks = string_list(arguments, "blocks"); }
                    if arguments.get("blockedBy").is_some() { task.blocked_by = string_list(arguments, "blockedBy"); }
                    if let Some(status) = arguments.get("status").and_then(Value::as_str) { task.status = match status { "pending" => WorkspaceTaskStatus::Pending, "in_progress" => WorkspaceTaskStatus::InProgress, "completed" => WorkspaceTaskStatus::Completed, _ => return json_error("status must be pending, in_progress, completed, or deleted") }; }
                }
            }
            _ => return json_error("unknown task tool"),
        }
        if let Err(err) = validate(&snapshot) { return json_error(err); }
        if let Err(err) = save_workspace_tasks(&self.session_id, &snapshot) { return json_error(err); }
        response(&snapshot, None)
    }
    fn execution_resource(&self, _: &Value) -> ToolExecutionResource { ToolExecutionResource::Exclusive }
}

fn string_list(arguments: &Value, key: &str) -> Vec<String> { arguments.get(key).and_then(Value::as_array).map(|items| items.iter().filter_map(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned).collect()).unwrap_or_default() }

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: WorkspaceTaskStatus, blocked_by: Vec<&str>) -> WorkspaceTask {
        WorkspaceTask {
            task_id: id.to_string(),
            subject: id.to_string(),
            description: String::new(),
            active_form: format!("working on {id}"),
            metadata: serde_json::json!({}),
            status,
            blocks: Vec::new(),
            blocked_by: blocked_by.into_iter().map(ToOwned::to_owned).collect(),
        }
    }

    #[test]
    fn validates_single_active_task_and_dependencies() {
        let snapshot = WorkspaceTaskSnapshot {
            tasks: vec![
                task("one", WorkspaceTaskStatus::Completed, vec![]),
                task("two", WorkspaceTaskStatus::InProgress, vec!["one"]),
            ],
        };
        assert!(validate(&snapshot).is_ok());

        let mut too_many = snapshot.clone();
        too_many.tasks[0].status = WorkspaceTaskStatus::InProgress;
        assert!(validate(&too_many).unwrap_err().contains("only one"));
    }

    #[test]
    fn rejects_blocked_and_cyclic_tasks() {
        let blocked = WorkspaceTaskSnapshot {
            tasks: vec![
                task("one", WorkspaceTaskStatus::Pending, vec![]),
                task("two", WorkspaceTaskStatus::InProgress, vec!["one"]),
            ],
        };
        assert!(validate(&blocked).unwrap_err().contains("blocked"));

        let cyclic = WorkspaceTaskSnapshot {
            tasks: vec![
                task("one", WorkspaceTaskStatus::Pending, vec!["two"]),
                task("two", WorkspaceTaskStatus::Pending, vec!["one"]),
            ],
        };
        assert!(validate(&cyclic).unwrap_err().contains("cycle"));
    }
}
