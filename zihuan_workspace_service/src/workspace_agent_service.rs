use std::path::{Path, PathBuf};
use std::sync::Arc;

use zihuan_core::agent::resource_resolver::resolve_local_embedding_model_name;
use zihuan_core::agent::resource_resolver::{build_llm_model, resolve_llm_service_config};
use zihuan_core::agent::service_config::{
    MemoryBackendKind, RoleServiceConfig, WorkspaceAgentServiceConfig,
};
use zihuan_core::agent::tools::Tool;
use zihuan_core::config::llm_refs::load_llm_refs;
use zihuan_core::graph::tool_spec::ToolDefinition;
use zihuan_core::memory_agent::{
    MemoryAgentResources, MemoryBackend, MemoryBrainAgent, MemoryBrainAgentTool,
};
use zihuan_core::model_inference::llm::{llm_base::LLMBase, LLMMessage};
use zihuan_core::model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use zihuan_core::runtime::block_async;
use zihuan_core::storage::{
    build_elasticsearch_ref, build_weaviate_ref, build_web_search_engine_ref,
    AgentMemoryAccessContext, ConnectionConfig, LocalMemoryStore, WeaviateCollectionSchema,
};
use zihuan_core::workspace::normalized_workspace_path;

use crate::tools::{
    AskUserTool, CopyFileTool, CreateFileTool, DeleteFileTool, EditFileTool, ExecCmdTool,
    FileInfoTool, FindFilesTool, GitStatusTool, GrepTool, ImageUnderstandTool, ListDirTool,
    MoveFileTool, ReadFileTool, RgTool, WorkspaceTaskTool, DEFAULT_TOOL_ASK_USER,
    DEFAULT_TOOL_COPY_FILE, DEFAULT_TOOL_CREATE_FILE, DEFAULT_TOOL_DELETE_FILE,
    DEFAULT_TOOL_EDIT_FILE, DEFAULT_TOOL_EXEC_CMD, DEFAULT_TOOL_FILE_INFO, DEFAULT_TOOL_FIND_FILES,
    DEFAULT_TOOL_GIT_STATUS, DEFAULT_TOOL_GREP, DEFAULT_TOOL_IMAGE_UNDERSTAND,
    DEFAULT_TOOL_LIST_DIR, DEFAULT_TOOL_MOVE_FILE, DEFAULT_TOOL_READ_FILE, DEFAULT_TOOL_RG,
    DEFAULT_TOOL_TASK_CREATE, DEFAULT_TOOL_TASK_GET, DEFAULT_TOOL_TASK_LIST,
    DEFAULT_TOOL_TASK_UPDATE, DEFAULT_TOOL_WEB_SEARCH,
};
use zihuan_core::agent::inference_provider::{InferenceToolContext, InferenceToolProvider};
use zihuan_core::agent::tool_definitions::build_enabled_tool_definitions;
use zihuan_core::agent::tools::WebSearchTool;
use zihuan_core::error::Result;

fn workspace_context_prompt(
    service_name: &str,
    workspace_path: &str,
    capabilities: &str,
) -> String {
    format!(
        "You are {service_name}, an assistant operating in the workspace directory: {workspace_path}\n\
         {capabilities}\nTask tracking: for any task requiring multiple meaningful steps, first create a concise task list with TaskCreate, keep exactly one task in progress, and mark tasks completed as work finishes."
    )
}

fn memory_prompt() -> &'static str {
    "[Memory] You can use memory_agent to recall relevant long-term information before answering. When this conversation establishes durable facts, preferences, decisions, or relationships, call memory_agent before finishing to save them. Do not save transient details, sensitive data, or information without long-term value."
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
    if is_enabled(enabled, DEFAULT_TOOL_WEB_SEARCH) {
        capabilities.push("search the web and read web pages");
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
    memory_resources: Option<WorkspaceMemoryResources>,
    web_search_engine: std::result::Result<Arc<dyn zihuan_core::rag::WebSearchEngine>, String>,
    tool_definitions: Vec<ToolDefinition>,
    image_understand_llm: Option<Arc<dyn LLMBase>>,
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
        if self.memory_resources.is_some() {
            let memory_prompt = memory_prompt().to_string();
            prompt = Some(match prompt {
                Some(prompt) => format!("{prompt}\n{memory_prompt}"),
                None => memory_prompt,
            });
        }
        if let Some(prompt) = prompt {
            messages.insert(0, LLMMessage::system(prompt));
        }
    }

    fn build_default_tools(&self, context: &InferenceToolContext) -> Vec<Box<dyn Tool>> {
        let workspace_path =
            normalized_workspace_path(context.workspace_path.as_deref()).map(PathBuf::from);
        let mut tools: Vec<Box<dyn Tool>> = Vec::new();
        if let Some(session_id) = context
            .session_id
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            for name in [
                DEFAULT_TOOL_TASK_CREATE,
                DEFAULT_TOOL_TASK_UPDATE,
                DEFAULT_TOOL_TASK_GET,
                DEFAULT_TOOL_TASK_LIST,
            ] {
                tools.push(Box::new(WorkspaceTaskTool::new(
                    session_id.to_string(),
                    name,
                )));
            }
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_CREATE_FILE) {
            tools.push(Box::new(CreateFileTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_DELETE_FILE) {
            tools.push(Box::new(DeleteFileTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_EDIT_FILE) {
            tools.push(Box::new(EditFileTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_READ_FILE) {
            tools.push(Box::new(ReadFileTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_LIST_DIR) {
            tools.push(Box::new(ListDirTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_GREP) {
            tools.push(Box::new(GrepTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_RG) {
            tools.push(Box::new(RgTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_FIND_FILES) {
            tools.push(Box::new(FindFilesTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_COPY_FILE) {
            tools.push(Box::new(CopyFileTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_MOVE_FILE) {
            tools.push(Box::new(MoveFileTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_FILE_INFO) {
            tools.push(Box::new(FileInfoTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_GIT_STATUS) {
            tools.push(Box::new(GitStatusTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_EXEC_CMD) {
            tools.push(Box::new(ExecCmdTool {
                workspace_path: workspace_path.clone(),
                session_id: context.session_id.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_ASK_USER) {
            tools.push(Box::new(AskUserTool));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_WEB_SEARCH) {
            let tool = match &self.web_search_engine {
                Ok(engine) => WebSearchTool::new(Arc::clone(engine)),
                Err(error) => WebSearchTool::unavailable(error.clone()),
            };
            tools.push(Box::new(tool));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_IMAGE_UNDERSTAND) {
            let image_llm = if let Some(image_understand_llm) = &context.image_understand_llm {
                Arc::clone(image_understand_llm)
            } else if context.llm.supports_multimodal_input() {
                Arc::clone(&context.llm)
            } else {
                self.image_understand_llm
                    .clone()
                    .unwrap_or_else(|| Arc::clone(&context.llm))
            };
            tools.push(Box::new(ImageUnderstandTool::new(
                context.image_media.clone(),
                image_llm,
            )));
        }
        if let Some(resources) = &self.memory_resources {
            tools.push(Box::new(MemoryBrainAgentTool::new(MemoryBrainAgent::new(
                resources.with_llm(Arc::clone(&context.llm)),
            ))));
        }
        tools
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool_definitions.clone()
    }
}

pub fn load_inference_tool_provider(
    agent: &RoleServiceConfig,
    config: &WorkspaceAgentServiceConfig,
    connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    let image_understand_llm =
        if is_enabled(&config.default_tools_enabled, DEFAULT_TOOL_IMAGE_UNDERSTAND) {
            let llm_ref_id = config
                .image_understand_llm_ref_id
                .as_deref()
                .or(config.llm_ref_id.as_deref());
            let llm_refs = load_llm_refs()?;
            let llm_config =
                resolve_llm_service_config(llm_ref_id, &llm_refs, DEFAULT_TOOL_IMAGE_UNDERSTAND)?;
            Some(build_llm_model(&llm_config)?)
        } else {
            None
        };
    Ok(Arc::new(WorkspaceInferenceToolProvider {
        service_name: agent.name.clone(),
        agents_md_enabled: config.agents_md_enabled,
        default_tools_enabled: config.default_tools_enabled.clone(),
        memory_resources: load_memory_resources(config, connections),
        web_search_engine: load_web_search_engine(config, connections),
        tool_definitions: build_enabled_tool_definitions(&agent.tools)?,
        image_understand_llm,
    }))
}

fn load_web_search_engine(
    config: &WorkspaceAgentServiceConfig,
    connections: &[ConnectionConfig],
) -> std::result::Result<Arc<dyn zihuan_core::rag::WebSearchEngine>, String> {
    let connection_id = config
        .web_search_engine_connection_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Web Search Engine connection is not configured".to_string())?;
    build_web_search_engine_ref(Some(connection_id), connections)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Web Search Engine connection is not configured".to_string())
}

#[derive(Clone)]
struct WorkspaceMemoryResources {
    memory_backend: MemoryBackend,
    embedding_model:
        Option<Arc<dyn zihuan_core::model_inference::llm::embedding_base::EmbeddingBase>>,
}

impl WorkspaceMemoryResources {
    fn with_llm(
        &self,
        llm: Arc<dyn zihuan_core::model_inference::llm::llm_base::LLMBase>,
    ) -> MemoryAgentResources {
        MemoryAgentResources {
            memory_backend: self.memory_backend.clone(),
            embedding_model: self.embedding_model.clone(),
            llm,
            access: AgentMemoryAccessContext::default(),
        }
    }
}

fn load_memory_resources(
    config: &WorkspaceAgentServiceConfig,
    connections: &[ConnectionConfig],
) -> Option<WorkspaceMemoryResources> {
    if !config.memory_enabled {
        return None;
    }

    let memory_backend = match config.memory_backend? {
        MemoryBackendKind::LocalFile => {
            MemoryBackend::LocalFile(Arc::new(LocalMemoryStore::in_app_data_dir()))
        }
        MemoryBackendKind::Weaviate => {
            let reference = build_weaviate_ref(
                config
                    .weaviate_memory_connection_id
                    .as_deref()
                    .filter(|value| !value.trim().is_empty()),
                connections,
                Some(WeaviateCollectionSchema::AgentMemory),
            )
            .ok()??;
            MemoryBackend::Weaviate(reference)
        }
        MemoryBackendKind::Elasticsearch => {
            let reference = build_elasticsearch_ref(
                config
                    .elasticsearch_memory_connection_id
                    .as_deref()
                    .filter(|value| !value.trim().is_empty()),
                connections,
                Some(WeaviateCollectionSchema::AgentMemory),
            )
            .ok()??;
            MemoryBackend::Elasticsearch(reference)
        }
    };

    let llm_refs = load_llm_refs().ok()?;
    let embedding_model = match config.memory_backend? {
        MemoryBackendKind::LocalFile => None,
        MemoryBackendKind::Weaviate | MemoryBackendKind::Elasticsearch => {
            let model_ref_id = config
                .embedding_model_ref_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())?;
            resolve_local_embedding_model_name(Some(model_ref_id), &llm_refs, "workspace")
                .ok()??;
            block_async(
                RuntimeEmbeddingModelManager::shared().get_or_create_embedding_model(model_ref_id),
            )
            .ok()
        }
    };

    Some(WorkspaceMemoryResources {
        memory_backend,
        embedding_model,
    })
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
    candidates.push(AgentsMdCandidate {
        location,
        path,
        exists,
    });
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
