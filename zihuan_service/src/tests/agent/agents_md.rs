use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use zihuan_core::agent::inference_provider::{InferenceToolContext, InferenceToolProvider};
use zihuan_core::inference::system_config::{AgentConfig, AgentType, WorkspaceAgentServiceConfig};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::{InferenceParam, LLMMessage, MessageRole};
use zihuan_workspace_agent::workspace_agent_service::load_inference_tool_provider;

static ENV_MUTEX: Mutex<()> = Mutex::new(());

struct TempDir { path: PathBuf }

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("zihuan-agents-md-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create temporary directory");
        Self { path }
    }
    fn agents_md_path(&self) -> PathBuf { self.path.join("AGENTS.md") }
    fn write_agents_md(&self, content: &str) { std::fs::write(self.agents_md_path(), content).expect("write AGENTS.md"); }
}

impl Drop for TempDir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.path); } }

fn provider(enabled: bool) -> Arc<dyn InferenceToolProvider> {
    let config = WorkspaceAgentServiceConfig {
        llm_ref_id: None,
        agents_md_enabled: enabled,
        memory_enabled: false,
        embedding_model_ref_id: None,
        weaviate_memory_connection_id: None,
        elasticsearch_memory_connection_id: None,
        memory_backend: None,
        web_search_engine_connection_id: None,
        default_tools_enabled: HashMap::new(),
    };
    let agent = AgentConfig {
        id: "test-agent".to_string(), config_id: "test-config".to_string(), name: "Test Agent".to_string(),
        agent_type: AgentType::Workspace(config.clone()), enabled: true, auto_start: false, is_default: false,
        updated_at: String::new(), tools: Vec::new(), avatar_url: None,
    };
    load_inference_tool_provider(&agent, &config, &[]).expect("load inference tool provider")
}

fn unused_llm() -> Arc<dyn LLMBase> {
    #[derive(Debug)]
    struct UnusedLlm;
    impl LLMBase for UnusedLlm {
        fn get_model_name(&self) -> &str {
            "test"
        }

        fn inference(&self, _param: &InferenceParam) -> LLMMessage {
            panic!("unused test LLM")
        }
    }
    Arc::new(UnusedLlm)
}

fn system_prompt(provider: &Arc<dyn InferenceToolProvider>, workspace_path: Option<String>) -> Option<String> {
    let mut messages = vec![LLMMessage::user("hello")];
    provider.augment_messages(&mut messages, &InferenceToolContext {
        last_user_text: "hello".to_string(),
        workspace_path,
        session_id: None,
        llm: unused_llm(),
    });
    messages.into_iter().find(|message| matches!(message.role, MessageRole::System)).and_then(|message| message.content_text().map(ToOwned::to_owned))
}

fn with_home<F, R>(home: &std::path::Path, action: F) -> R where F: FnOnce() -> R {
    let _guard = ENV_MUTEX.lock().expect("environment mutex poisoned");
    let previous_home = std::env::var_os("HOME"); let previous_userprofile = std::env::var_os("USERPROFILE");
    std::env::set_var("HOME", home); std::env::set_var("USERPROFILE", home);
    let result = action();
    match previous_home { Some(value) => std::env::set_var("HOME", value), None => std::env::remove_var("HOME") }
    match previous_userprofile { Some(value) => std::env::set_var("USERPROFILE", value), None => std::env::remove_var("USERPROFILE") }
    result
}

/// Purpose: Verify enabled workspace configuration references its AGENTS.md file.
/// TestData: A workspace containing AGENTS.md and an enabled provider.
#[test]
fn agents_md_enabled_workspace_file_references_agents_md() {
    let workspace = TempDir::new(); workspace.write_agents_md("# Project rules");
    let prompt = system_prompt(&provider(true), Some(workspace.path.to_string_lossy().to_string())).expect("system prompt");
    let expected = std::fs::canonicalize(workspace.agents_md_path()).expect("canonical workspace AGENTS.md");
    assert!(prompt.contains("AGENTS.md")); assert!(prompt.contains("read_file")); assert!(prompt.contains(&format!("- {}", expected.display())));
}

/// Purpose: Verify disabled workspace configuration omits AGENTS.md references.
/// TestData: A workspace containing AGENTS.md and a disabled provider.
#[test]
fn agents_md_disabled_does_not_reference_agents_md() {
    let workspace = TempDir::new(); workspace.write_agents_md("# Project rules");
    let prompt = system_prompt(&provider(false), Some(workspace.path.to_string_lossy().to_string())).expect("system prompt");
    assert!(!prompt.contains("AGENTS.md")); assert!(prompt.contains("workspace directory"));
}

/// Purpose: Verify absent workspace and home files produce no AGENTS.md reference.
/// TestData: Empty temporary workspace and home directories.
#[test]
fn agents_md_enabled_missing_file_does_not_reference_agents_md() {
    let workspace = TempDir::new();
    let prompt = with_home(&workspace.path, || system_prompt(&provider(true), Some(workspace.path.to_string_lossy().to_string()))).expect("system prompt");
    assert!(!prompt.contains("AGENTS.md"));
}

/// Purpose: Verify home AGENTS.md is used when the workspace has none.
/// TestData: A home directory containing AGENTS.md and an empty workspace.
#[test]
fn agents_md_enabled_home_file_references_agents_md() {
    let home = TempDir::new(); home.write_agents_md("# Home rules"); let workspace = TempDir::new();
    let prompt = with_home(&home.path, || system_prompt(&provider(true), Some(workspace.path.to_string_lossy().to_string()))).expect("system prompt");
    let expected = std::fs::canonicalize(home.agents_md_path()).expect("canonical home AGENTS.md");
    assert!(prompt.contains(&format!("- {}", expected.display())));
}

/// Purpose: Verify workspace AGENTS.md takes priority over the home file.
/// TestData: Workspace and home directories each containing AGENTS.md.
#[test]
fn agents_md_enabled_multiple_files_lists_in_priority_order() {
    let workspace = TempDir::new(); workspace.write_agents_md("# Workspace rules"); let home = TempDir::new(); home.write_agents_md("# Home rules");
    let workspace_path = workspace.path.to_string_lossy().to_string();
    let prompt = with_home(&home.path, || system_prompt(&provider(true), Some(workspace_path))).expect("system prompt");
    let workspace_line = format!("- {}", std::fs::canonicalize(workspace.agents_md_path()).expect("canonical workspace").display());
    let home_line = format!("- {}", std::fs::canonicalize(home.agents_md_path()).expect("canonical home").display());
    assert!(prompt.find(&workspace_line).expect("workspace line") < prompt.find(&home_line).expect("home line"));
}
