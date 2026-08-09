//! Integration tests for Workspace Agent AGENTS.md loading.
//!
//! These tests verify that `WorkspaceInferenceToolProvider` injects the correct AGENTS.md
//! references into the system prompt under different file-location and configuration
//! combinations.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use zihuan_core::inference::system_config::{AgentConfig, AgentType, WorkspaceAgentServiceConfig};
use zihuan_core::llm::{LLMMessage, MessageRole};
use zihuan_service::agent::inference::{InferenceToolContext, InferenceToolProvider};
use zihuan_workspace_agent::workspace_agent_service::load_inference_tool_provider;

/// Serialize tests that mutate process-level environment variables.
static ENV_MUTEX: Mutex<()> = Mutex::new(());

/// Temporary directory that creates an `AGENTS.md` file and cleans itself up on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("zihuan-agents-md-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("failed to create temp directory");
        Self { path }
    }

    fn agents_md_path(&self) -> PathBuf {
        self.path.join("AGENTS.md")
    }

    fn write_agents_md(&self, content: &str) {
        std::fs::write(self.agents_md_path(), content).expect("failed to write AGENTS.md");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Build a Workspace inference tool provider with the requested AGENTS.md flag and no tools.
fn build_provider(agents_md_enabled: bool) -> std::sync::Arc<dyn InferenceToolProvider> {
    let workspace_config = WorkspaceAgentServiceConfig {
        llm_ref_id: None,
        agents_md_enabled,
        default_tools_enabled: HashMap::new(),
    };
    let agent = AgentConfig {
        id: "test-agent".to_string(),
        config_id: "test-config".to_string(),
        name: "Test Agent".to_string(),
        agent_type: AgentType::Workspace(workspace_config.clone()),
        enabled: true,
        auto_start: false,
        is_default: false,
        updated_at: String::new(),
        tools: Vec::new(),
        avatar_url: None,
    };
    load_inference_tool_provider(&agent, &workspace_config, &[])
        .expect("failed to load inference tool provider")
}

/// Run augmentation and return the content of the first system message, if any.
fn augment_and_get_system_prompt(
    provider: &std::sync::Arc<dyn InferenceToolProvider>,
    workspace_path: Option<String>,
) -> Option<String> {
    let mut messages = vec![LLMMessage::user("hello")];
    let context = InferenceToolContext {
        last_user_text: "hello".to_string(),
        workspace_path,
    };
    provider.augment_messages(&mut messages, &context);
    messages
        .into_iter()
        .find(|message| matches!(message.role, MessageRole::System))
        .and_then(|message| message.content_text().map(ToOwned::to_owned))
}

/// Purpose: Verify that the system prompt mentions AGENTS.md and the workspace file path
/// when the feature is enabled and the file exists.
///
/// Test Data: A workspace directory containing `AGENTS.md`; provider configured with
/// `agents_md_enabled = true`.
///
/// Test Measure: The injected system message contains the substring "AGENTS.md", the
/// `read_file` instruction, and a canonical path line matching the workspace AGENTS.md.
#[test]
fn agents_md_enabled_workspace_file_references_agents_md() {
    let workspace = TempDir::new();
    workspace.write_agents_md("# Project rules");
    let provider = build_provider(true);
    let workspace_path = workspace.path.to_string_lossy().to_string();

    let prompt = augment_and_get_system_prompt(&provider, Some(workspace_path))
        .expect("system prompt should be injected");

    assert!(prompt.contains("AGENTS.md"), "prompt should mention AGENTS.md");
    assert!(prompt.contains("read_file"), "prompt should instruct the agent to read AGENTS.md");
    let expected_canonical = std::fs::canonicalize(workspace.agents_md_path())
        .expect("failed to canonicalize workspace AGENTS.md");
    assert!(
        prompt.contains(&format!("- {}", expected_canonical.display())),
        "prompt should list the workspace AGENTS.md path"
    );
}

/// Purpose: Verify that AGENTS.md references are omitted when the feature is disabled,
/// even if a workspace AGENTS.md file exists.
///
/// Test Data: A workspace directory containing `AGENTS.md`; provider configured with
/// `agents_md_enabled = false`.
///
/// Test Measure: The system prompt does not contain "AGENTS.md" or the workspace
/// AGENTS.md path, while still describing the workspace context.
#[test]
fn agents_md_disabled_does_not_reference_agents_md() {
    let workspace = TempDir::new();
    workspace.write_agents_md("# Project rules");
    let provider = build_provider(false);
    let workspace_path = workspace.path.to_string_lossy().to_string();

    let prompt = augment_and_get_system_prompt(&provider, Some(workspace_path))
        .expect("workspace context prompt should be injected");

    assert!(!prompt.contains("AGENTS.md"), "prompt should not mention AGENTS.md when disabled");
    assert!(
        !prompt.contains(&format!("- {}", workspace.agents_md_path().display())),
        "prompt should not list the AGENTS.md path when disabled"
    );
    assert!(prompt.contains("workspace directory"), "prompt should still describe the workspace");
}

/// Purpose: Verify that no AGENTS.md reference is injected when the feature is enabled
/// but no AGENTS.md file exists in any candidate location.
///
/// Test Data: A workspace directory without `AGENTS.md`; provider configured with
/// `agents_md_enabled = true`.
///
/// Test Measure: The system prompt does not contain "AGENTS.md".
#[test]
fn agents_md_enabled_missing_file_does_not_reference_agents_md() {
    let workspace = TempDir::new();
    let provider = build_provider(true);
    let workspace_path = workspace.path.to_string_lossy().to_string();

    let prompt = augment_and_get_system_prompt(&provider, Some(workspace_path))
        .expect("workspace context prompt should be injected");

    assert!(!prompt.contains("AGENTS.md"), "prompt should not mention AGENTS.md when file is absent");
}

/// Purpose: Verify that a home-directory AGENTS.md is referenced when no workspace file
/// exists and the feature is enabled.
///
/// Test Data: A temporary home directory containing `AGENTS.md`; the `HOME` and
/// `USERPROFILE` environment variables are redirected to that directory.
///
/// Test Measure: The system prompt contains "AGENTS.md" and the canonical home-directory
/// AGENTS.md path.
#[test]
fn agents_md_enabled_home_file_references_agents_md() {
    let home = TempDir::new();
    home.write_agents_md("# Home-level rules");
    let workspace = TempDir::new();
    let provider = build_provider(true);

    let result = with_home_dir(&home.path, || {
        augment_and_get_system_prompt(&provider, Some(workspace.path.to_string_lossy().to_string()))
    });

    let prompt = result.expect("system prompt should be injected");
    assert!(prompt.contains("AGENTS.md"), "prompt should mention AGENTS.md from home dir");
    let expected_canonical = std::fs::canonicalize(home.agents_md_path())
        .expect("failed to canonicalize home AGENTS.md");
    assert!(
        prompt.contains(&format!("- {}", expected_canonical.display())),
        "prompt should list the home AGENTS.md path"
    );
}

/// Purpose: Verify that AGENTS.md files from multiple locations are listed in priority
/// order: workspace first, then home.
///
/// Test Data: A workspace directory containing `AGENTS.md` and a temporary home directory
/// containing `AGENTS.md`; `HOME`/`USERPROFILE` redirected to the temporary home.
///
/// Test Measure: The system prompt contains both AGENTS.md paths, and the workspace path
/// appears before the home path.
#[test]
fn agents_md_enabled_multiple_files_lists_in_priority_order() {
    let workspace = TempDir::new();
    workspace.write_agents_md("# Workspace rules");
    let home = TempDir::new();
    home.write_agents_md("# Home rules");
    let provider = build_provider(true);
    let workspace_path = workspace.path.to_string_lossy().to_string();

    let result = with_home_dir(&home.path, || {
        augment_and_get_system_prompt(&provider, Some(workspace_path.clone()))
    });

    let prompt = result.expect("system prompt should be injected");
    let workspace_canonical = std::fs::canonicalize(workspace.agents_md_path())
        .expect("failed to canonicalize workspace AGENTS.md");
    let home_canonical = std::fs::canonicalize(home.agents_md_path())
        .expect("failed to canonicalize home AGENTS.md");
    let workspace_line = format!("- {}", workspace_canonical.display());
    let home_line = format!("- {}", home_canonical.display());

    assert!(prompt.contains(&workspace_line), "prompt should list workspace AGENTS.md");
    assert!(prompt.contains(&home_line), "prompt should list home AGENTS.md");
    assert!(
        prompt.find(&workspace_line).expect("workspace line missing") <
            prompt.find(&home_line).expect("home line missing"),
        "workspace AGENTS.md should appear before home AGENTS.md"
    );
}

/// Run `action` with `HOME` and `USERPROFILE` temporarily redirected to `home_dir`, then
/// restore the previous values.
fn with_home_dir<F, R>(home_dir: &std::path::Path, action: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = ENV_MUTEX.lock().expect("env mutex poisoned");
    let previous_home = std::env::var_os("HOME");
    let previous_userprofile = std::env::var_os("USERPROFILE");
    std::env::set_var("HOME", home_dir);
    std::env::set_var("USERPROFILE", home_dir);

    let result = action();

    match previous_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    match previous_userprofile {
        Some(value) => std::env::set_var("USERPROFILE", value),
        None => std::env::remove_var("USERPROFILE"),
    }

    result
}
