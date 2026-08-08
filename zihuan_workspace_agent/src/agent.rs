use std::path::{Path, PathBuf};
use std::sync::Arc;

use zihuan_core::model_inference::system_config::{AgentConfig, WorkspaceAgentServiceConfig};
use zihuan_core::storage_handler::ConnectionConfig;
use zihuan_core::agent::brain::BrainTool;
use zihuan_core::llm::LLMMessage;
use zihuan_core::workspace::normalized_workspace_path;
use zihuan_core::graph_engine::brain_tool_spec::BrainToolDefinition;

use zihuan_core::agent::inference::{InferenceToolContext, InferenceToolProvider};
use zihuan_core::agent::tool_definitions::build_enabled_tool_definitions;
use crate::tools::{
    AskUserBrainTool, CreateFileBrainTool, DeleteFileBrainTool, EditFileBrainTool, ExecCmdBrainTool,
    CopyFileBrainTool, FileInfoBrainTool, FindFilesBrainTool, GitStatusBrainTool, GrepBrainTool, ListDirBrainTool, MoveFileBrainTool, ReadFileBrainTool, RgBrainTool, DEFAULT_TOOL_ASK_USER,
    DEFAULT_TOOL_CREATE_FILE, DEFAULT_TOOL_DELETE_FILE, DEFAULT_TOOL_EDIT_FILE, DEFAULT_TOOL_EXEC_CMD,
    DEFAULT_TOOL_COPY_FILE, DEFAULT_TOOL_FILE_INFO, DEFAULT_TOOL_FIND_FILES, DEFAULT_TOOL_GIT_STATUS, DEFAULT_TOOL_GREP, DEFAULT_TOOL_LIST_DIR, DEFAULT_TOOL_MOVE_FILE, DEFAULT_TOOL_READ_FILE, DEFAULT_TOOL_RG,
};
use zihuan_core::error::Result;

// Prompt engineering


fn workspace_context_prompt(service_name: &str, workspace_path: &str, capabilities: &str) -> String {
    format!(
        "You are {service_name}, an assistant operating in the workspace directory: {workspace_path}\n\
         {capabilities}"
    )
}

fn build_tool_capabilities(enabled: &std::collections::HashMap<String, bool>) -> String {
    let mut capabilities = Vec::new();
    if is_enabled(enabled, DEFAULT_TOOL_READ_FILE) {
        capabilities.push("read files (read_file can read binary fragments as base64 when needed)");
    }
    if is_enabled(enabled, DEFAULT_TOOL_LIST_DIR) {
        capabilities.push("list directories");
    }
    if is_enabled(enabled, DEFAULT_TOOL_FIND_FILES) {
        capabilities.push("find paths by name");
    }
    if is_enabled(enabled, DEFAULT_TOOL_GREP) || is_enabled(enabled, DEFAULT_TOOL_RG) {
        capabilities.push("search text");
    }
    if is_enabled(enabled, DEFAULT_TOOL_CREATE_FILE) {
        capabilities.push("create files");
    }
    if is_enabled(enabled, DEFAULT_TOOL_EDIT_FILE) {
        capabilities.push("edit files");
    }
    if is_enabled(enabled, DEFAULT_TOOL_DELETE_FILE) {
        capabilities.push("delete files");
    }
    if is_enabled(enabled, DEFAULT_TOOL_COPY_FILE) {
        capabilities.push("copy files");
    }
    if is_enabled(enabled, DEFAULT_TOOL_MOVE_FILE) {
        capabilities.push("move files");
    }
    if is_enabled(enabled, DEFAULT_TOOL_FILE_INFO) {
        capabilities.push("view metadata");
    }
    if is_enabled(enabled, DEFAULT_TOOL_GIT_STATUS) {
        capabilities.push("view Git status");
    }
    if is_enabled(enabled, DEFAULT_TOOL_EXEC_CMD) {
        capabilities.push("execute commands");
    }
    if is_enabled(enabled, DEFAULT_TOOL_ASK_USER) {
        capabilities.push("ask the user for clarification");
    }
    if capabilities.is_empty() {
        "You have no workspace tools enabled.".to_string()
    } else {
        format!("You can {}.", capabilities.join(", "))
    }
}

fn agents_md_prompt(references: &str) -> String {
    format!(
        "[Mandatory Requirement] Before starting any work, you must use read_file to read each of the following AGENTS.md files one by one, and strictly follow the project rules, coding standards, architecture conventions, and build commands within them. These files are the highest-priority working constraints for this project: their rules must be followed unconditionally, and if they conflict with general rules or default assumptions, AGENTS.md always takes precedence. You must not create, modify, or delete any files until you have finished reading them. AGENTS.md files to read and follow:\n{references}"
    )
}

// ===================================

pub struct WorkspaceInferenceToolProvider {
    service_name: String,
    agents_md_enabled: bool,
    default_tools_enabled: std::collections::HashMap<String, bool>,
    tool_definitions: Vec<BrainToolDefinition>,
}

impl InferenceToolProvider for WorkspaceInferenceToolProvider {
    fn augment_messages(&self, messages: &mut Vec<LLMMessage>, context: &InferenceToolContext) {
        let capabilities = build_tool_capabilities(&self.default_tools_enabled);
        let mut prompt = context
            .workspace_path
            .as_ref()
            .map(|path| workspace_context_prompt(&self.service_name, path, &capabilities));
        if self.agents_md_enabled {
            let agents_paths = discover_agents_md_paths(context.workspace_path.as_deref());
            if !agents_paths.is_empty() {
                let references = agents_paths
                    .iter()
                    .map(|path| format!("- {}", path.display()))
                    .collect::<Vec<_>>()
                    .join("\n");
                let agents_prompt = agents_md_prompt(&references);
                prompt = Some(match prompt {
                    Some(prompt) => format!("{prompt}\n{agents_prompt}"),
                    None => agents_prompt,
                });
            }
        }
        if let Some(prompt) = prompt {
            messages.insert(0, LLMMessage::system(prompt));
        }
    }

    fn build_default_tools(&self, context: &InferenceToolContext) -> Vec<Box<dyn BrainTool>> {
        let workspace_path = normalized_workspace_path(context.workspace_path.as_deref()).map(PathBuf::from);
        let mut tools: Vec<Box<dyn BrainTool>> = Vec::new();
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_CREATE_FILE) {
            tools.push(Box::new(CreateFileBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_DELETE_FILE) {
            tools.push(Box::new(DeleteFileBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_EDIT_FILE) {
            tools.push(Box::new(EditFileBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_READ_FILE) {
            tools.push(Box::new(ReadFileBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_LIST_DIR) {
            tools.push(Box::new(ListDirBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_GREP) {
            tools.push(Box::new(GrepBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_RG) {
            tools.push(Box::new(RgBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_FIND_FILES) {
            tools.push(Box::new(FindFilesBrainTool { workspace_path: workspace_path.clone() }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_COPY_FILE) {
            tools.push(Box::new(CopyFileBrainTool { workspace_path: workspace_path.clone() }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_MOVE_FILE) {
            tools.push(Box::new(MoveFileBrainTool { workspace_path: workspace_path.clone() }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_FILE_INFO) {
            tools.push(Box::new(FileInfoBrainTool { workspace_path: workspace_path.clone() }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_GIT_STATUS) {
            tools.push(Box::new(GitStatusBrainTool { workspace_path: workspace_path.clone() }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_EXEC_CMD) {
            tools.push(Box::new(ExecCmdBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_ASK_USER) {
            tools.push(Box::new(AskUserBrainTool));
        }
        tools
    }

    fn tool_definitions(&self) -> Vec<BrainToolDefinition> {
        self.tool_definitions.clone()
    }
}

pub fn load_inference_tool_provider(
    agent: &AgentConfig,
    config: &WorkspaceAgentServiceConfig,
    _connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    Ok(Arc::new(WorkspaceInferenceToolProvider {
        service_name: agent.name.clone(),
        agents_md_enabled: config.agents_md_enabled,
        default_tools_enabled: config.default_tools_enabled.clone(),
        tool_definitions: build_enabled_tool_definitions(&agent.tools)?,
    }))
}

/// Location of an `AGENTS.md` candidate, listed in discovery priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentsMdLocation {
    Workspace,
    Executable,
    Home,
}

impl AgentsMdLocation {
    /// Stable key used by the management API and the frontend.
    pub fn key(self) -> &'static str {
        match self {
            AgentsMdLocation::Workspace => "workspace",
            AgentsMdLocation::Executable => "executable",
            AgentsMdLocation::Home => "home",
        }
    }
}

/// One `AGENTS.md` candidate exposed to the management API.
#[derive(Debug, Clone)]
pub struct AgentsMdCandidate {
    pub location: AgentsMdLocation,
    pub path: PathBuf,
    pub exists: bool,
}

/// Lists the candidate `AGENTS.md` files across the workspace, executable, and user home
/// directories in priority order. When no workspace path is provided, the current process
/// directory is used as the workspace fallback, matching the effective workspace resolution
/// used at inference time. Directories that cannot be resolved are skipped.
pub fn agents_md_candidates(workspace_path: Option<&str>) -> Vec<AgentsMdCandidate> {
    let workspace_dir = workspace_path
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| std::env::current_dir().ok());
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let home_dir = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    let mut candidates = Vec::new();
    if let Some(directory) = workspace_dir {
        push_agents_md_candidate(&mut candidates, AgentsMdLocation::Workspace, directory);
    }
    if let Some(directory) = executable_dir {
        push_agents_md_candidate(&mut candidates, AgentsMdLocation::Executable, directory);
    }
    if let Some(directory) = home_dir {
        push_agents_md_candidate(&mut candidates, AgentsMdLocation::Home, directory);
    }
    candidates
}

fn push_agents_md_candidate(
    candidates: &mut Vec<AgentsMdCandidate>,
    location: AgentsMdLocation,
    directory: PathBuf,
) {
    let path = directory.join("AGENTS.md");
    let exists = path.is_file();
    candidates.push(AgentsMdCandidate { location, path, exists });
}

/// Returns the existing `AGENTS.md` paths in priority order, deduplicated by canonical path.
pub fn discover_agents_md_paths(workspace_path: Option<&str>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for candidate in agents_md_candidates(workspace_path) {
        if !candidate.exists {
            continue;
        }
        let Ok(path) = std::fs::canonicalize(&candidate.path) else {
            continue;
        };
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

fn is_enabled(map: &std::collections::HashMap<String, bool>, name: &str) -> bool {
    *map.get(name).unwrap_or(&true)
}
