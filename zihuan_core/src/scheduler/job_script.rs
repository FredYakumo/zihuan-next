//! Scheduler job scripts: every script under `scheduled_jobs/` declares its own manifest
//! inline (`job_manifest` export for `.mjs`, `JOB_MANIFEST` dict for `.py`) and the script
//! runners report those manifests through the `--jobs-catalog` catalog mode.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use dynamic_script_engine::{NodeRuntimeConfig, PythonRuntimeConfig, ScriptLanguage};

use super::capabilities;
use super::{JobRunContext, SchedulerJob};
use crate::config::ConfigCenter;
use crate::error::{Error, Result};

/// Task name of the built-in Dream consolidation job (`scheduled_jobs/dream_job.py`).
pub const DREAM_TASK_NAME: &str = "Dream";

/// Directory holding scheduler job scripts, relative to the workspace.
pub fn scheduled_jobs_dir() -> PathBuf {
    PathBuf::from("scheduled_jobs")
}

/// Built-in job scripts shipped with the application, embedded so a fresh install can seed
/// its `scheduled_jobs` directory without any Rust-side job body.
const BUILTIN_JOB_SCRIPTS: &[(&str, &str)] = &[(
    "dream_job",
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../scheduled_jobs/dream_job.py")),
)];

#[derive(Debug, Clone)]
pub struct JobManifest {
    pub task_name: String,
    pub script: String,
    pub entry: String,
    pub description: String,
}

impl JobManifest {
    fn language(&self) -> Result<ScriptLanguage> {
        let path = scheduled_jobs_dir().join(&self.script);
        if !path.is_file() {
            return Err(Error::ValidationError(format!("job 脚本不存在: {}", path.display())));
        }
        ScriptLanguage::from_path(&path).ok_or_else(|| {
            Error::ValidationError(format!("job 脚本必须是 .mjs 或 .py 文件: {}", path.display()))
        })
    }
}

fn manifest_from_entry(entry: dynamic_script_engine::JobCatalogEntry) -> Result<JobManifest> {
    if entry.task_name.trim().is_empty() {
        return Err(Error::ValidationError("job 清单 task_name 不能为空".to_string()));
    }
    Ok(JobManifest {
        task_name: entry.task_name,
        script: entry.script,
        entry: entry.entry,
        description: entry.description,
    })
}

/// Creates the built-in job scripts that are absent at application startup.
/// Existing files are intentionally left untouched.
pub fn ensure_default_jobs() -> Result<()> {
    let directory = scheduled_jobs_dir();
    fs::create_dir_all(&directory).map_err(|error| {
        Error::ValidationError(format!("failed to create scheduled jobs directory: {error}"))
    })?;
    for (name, content) in BUILTIN_JOB_SCRIPTS {
        let path = directory.join(format!("{name}.py"));
        if path.exists() {
            continue;
        }
        fs::write(&path, content).map_err(|error| {
            Error::ValidationError(format!("failed to write default job script '{name}': {error}"))
        })?;
    }
    Ok(())
}

/// Seeds default job files, collects the manifests the script runners report from
/// `scheduled_jobs/`, and registers a script job body per `task_name`. Per-job failures are
/// logged and skipped so one broken job cannot hide the others.
pub fn init_script_jobs() -> Result<usize> {
    ensure_default_jobs()?;
    let workspace = std::env::current_dir()
        .map_err(|error| Error::ValidationError(format!("failed to resolve workspace: {error}")))?;
    let (node_runtime, python_runtime) = runtime_configs()?;
    let catalog = dynamic_script_engine::load_job_catalog(
        &workspace,
        &scheduled_jobs_dir(),
        &node_runtime,
        &python_runtime,
    )
    .map_err(|error| crate::string_error!("加载 scheduled_jobs 目录失败: {error}"))?;
    for diagnostic in &catalog.diagnostics {
        log::warn!("[Scheduler] job 目录诊断: {}", diagnostic.message);
    }
    // `task_name` -> the script that already claimed it, so a duplicate can name both sides.
    let mut registered_scripts: HashMap<String, String> = HashMap::new();
    let mut count = 0;
    for entry in catalog.jobs {
        let manifest = match manifest_from_entry(entry) {
            Ok(manifest) => manifest,
            Err(error) => {
                log::warn!("[Scheduler] {}", error);
                continue;
            }
        };
        if let Some(owner) = registered_scripts.get(&manifest.task_name) {
            log::error!(
                "[Scheduler] job task_name 重复 {}（task_name '{}' 已由 {} 注册）",
                manifest.script,
                manifest.task_name,
                owner
            );
            continue;
        }
        let language = match manifest.language() {
            Ok(language) => language,
            Err(error) => {
                log::warn!("[Scheduler] {}", error);
                continue;
            }
        };
        if let Err(error) = dynamic_script_engine::start_script_runtime(
            &workspace,
            language,
            &node_runtime,
            &python_runtime,
        ) {
            log::warn!("[Scheduler] failed to start script runtime for {language:?} jobs: {error}");
        }
        let task_name = manifest.task_name.clone();
        let script = manifest.script.clone();
        super::register_job(manifest.clone(), Arc::new(ScriptJob { manifest, language }));
        registered_scripts.insert(task_name, script);
        count += 1;
    }
    Ok(count)
}

fn runtime_configs() -> Result<(NodeRuntimeConfig, PythonRuntimeConfig)> {
    let root = ConfigCenter::shared().load_root()?;
    Ok((root.node_runtime, root.python_runtime))
}

struct ScriptJob {
    manifest: JobManifest,
    language: ScriptLanguage,
}

impl SchedulerJob for ScriptJob {
    fn run(&self, context: &JobRunContext) -> Result<Option<String>> {
        let workspace = std::env::current_dir().map_err(|error| {
            Error::ValidationError(format!("failed to resolve workspace: {error}"))
        })?;
        let (node_runtime, python_runtime) = runtime_configs()?;
        let script_path = scheduled_jobs_dir().join(&self.manifest.script);
        let request = json!({
            "task": {
                "id": context.task.id,
                "task_name": context.task.task_name,
                "source_service": context.task.source_service,
                "triggered_by": context.task.triggered_by,
                "start_time": context.task.start_time.to_rfc3339(),
            },
            "agent_id": context.resources.agent_id,
            "sender_id": context.task.triggered_by,
        });
        let resources = Arc::clone(&context.resources);
        let response = dynamic_script_engine::execute_script_tool(
            &workspace,
            self.language,
            &node_runtime,
            &python_runtime,
            &script_path,
            &self.manifest.entry,
            &request,
            &mut |method: &str, params: &Value| capabilities::dispatch(&resources, method, params),
        )
        .map_err(|error| crate::string_error!("job script 执行失败: {error}"))?;
        let ok = response
            .get("ok")
            .and_then(Value::as_bool)
            .ok_or_else(|| Error::ValidationError("job script 响应缺少 ok 布尔值".to_string()))?;
        if !ok {
            let error = response.get("error").and_then(Value::as_str).unwrap_or("未知错误");
            return Err(Error::ValidationError(format!("job script 执行失败: {error}")));
        }
        Ok(response.get("result").and_then(Value::as_str).map(str::to_string))
    }
}
