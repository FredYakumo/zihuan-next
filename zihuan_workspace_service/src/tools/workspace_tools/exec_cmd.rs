use super::shared::{json_error, resolve_tool_path, success_json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use zihuan_core::agent::tools::{Tool, ToolExecutionOutput, ToolExecutionResource};
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::model_inference::llm::tooling::StaticFunctionToolSpec;
use zihuan_core::runtime::block_async;
use zihuan_core::utils::string_utils::truncate_output;
pub(crate) const DEFAULT_TOOL_EXEC_CMD: &str = "exec_cmd";
#[derive(Debug, Deserialize)]
struct ExecCmdArgs {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    input: Option<String>,
    #[serde(default)]
    max_output_bytes: Option<usize>,
    #[serde(default)]
    shell: Option<String>,
}
#[derive(Debug, Clone)]
pub(crate) struct ExecCmdTool {
    pub(crate) workspace_path: Option<PathBuf>,
    pub(crate) session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingCommandApproval {
    pub command: String,
    pub shell: String,
}

#[derive(Default)]
struct CommandApprovals {
    families: Vec<(String, String)>,
    decisions: HashMap<(String, String), bool>,
    pending: HashMap<String, PendingCommandApproval>,
}
static COMMAND_APPROVALS: OnceLock<(Mutex<CommandApprovals>, Condvar)> = OnceLock::new();

pub fn approve_command(session_id: &str, command: &str, allow_similar: bool) -> bool {
    let (lock, wake) =
        COMMAND_APPROVALS.get_or_init(|| (Mutex::new(CommandApprovals::default()), Condvar::new()));
    let mut approvals = lock.lock().unwrap();
    let Some(pending) = approvals.pending.get(session_id) else {
        return false;
    };
    if pending.command != command {
        return false;
    }
    if allow_similar {
        approvals.families.push((session_id.to_string(), command_family(command)))
    }
    approvals.decisions.insert((session_id.to_string(), command.to_string()), true);
    wake.notify_all();
    true
}

pub fn reject_command(session_id: &str, command: &str) -> bool {
    let (lock, wake) =
        COMMAND_APPROVALS.get_or_init(|| (Mutex::new(CommandApprovals::default()), Condvar::new()));
    let mut approvals = lock.lock().unwrap();
    let Some(pending) = approvals.pending.get(session_id) else {
        return false;
    };
    if pending.command != command {
        return false;
    }
    approvals.decisions.insert((session_id.to_string(), command.to_string()), false);
    wake.notify_all();
    true
}

pub fn pending_command_approval(session_id: &str) -> Option<PendingCommandApproval> {
    let (lock, _) =
        COMMAND_APPROVALS.get_or_init(|| (Mutex::new(CommandApprovals::default()), Condvar::new()));
    lock.lock().unwrap().pending.get(session_id).cloned()
}

fn is_approved(session_id: Option<&str>, command: &str) -> bool {
    let Some(session_id) = session_id else {
        return false;
    };
    let (lock, _) =
        COMMAND_APPROVALS.get_or_init(|| (Mutex::new(CommandApprovals::default()), Condvar::new()));
    let approvals = lock.lock().unwrap();
    let family = command_family(command);
    approvals
        .families
        .iter()
        .any(|(session, allowed)| session == session_id && allowed == &family)
}

fn wait_for_command_decision(session_id: &str, command: &str) -> bool {
    let (lock, wake) =
        COMMAND_APPROVALS.get_or_init(|| (Mutex::new(CommandApprovals::default()), Condvar::new()));
    let key = (session_id.to_string(), command.to_string());
    let mut approvals = lock.lock().unwrap();
    while !approvals.decisions.contains_key(&key) {
        approvals = wake.wait(approvals).unwrap();
    }
    let decision = approvals.decisions.remove(&key).unwrap_or(false);
    approvals.pending.remove(session_id);
    decision
}

fn command_family(command: &str) -> String {
    command.split_whitespace().next().unwrap_or_default().to_ascii_lowercase()
}
impl Tool for ExecCmdTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec{name:DEFAULT_TOOL_EXEC_CMD,description:"Execute a shell command with optional environment, stdin, shell, timeout, and bounded output",parameters:serde_json::json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"},"timeout_secs":{"type":"integer","minimum":1},"env":{"type":"object","additionalProperties":{"type":"string"}},"input":{"type":"string"},"max_output_bytes":{"type":"integer","minimum":1},"shell":{"type":"string","enum":["powershell","bash"]}},"required":["command"]})})
    }
    fn execute(&self, call_content: &str, a: &Value) -> String {
        self.execute_with_progress(call_content, a, Arc::new(|_, _| {})).result
    }
    fn execute_with_progress(
        &self,
        _: &str,
        a: &Value,
        on_output: Arc<dyn Fn(&str, &str) + Send + Sync>,
    ) -> ToolExecutionOutput {
        let args: ExecCmdArgs = match serde_json::from_value(a.clone()) {
            Ok(v) => v,
            Err(e) => {
                return ToolExecutionOutput::text(json_error(format!(
                    "invalid exec_cmd arguments: {e}"
                )))
            }
        };
        let shell = args.shell.clone().unwrap_or_else(|| {
            if cfg!(windows) {
                "powershell".to_string()
            } else {
                "bash".to_string()
            }
        });
        if shell != "powershell" && shell != "bash" {
            return ToolExecutionOutput::text(json_error("shell must be powershell or bash"));
        }
        if !is_approved(self.session_id.as_deref(), &args.command) {
            let Some(session_id) = self.session_id.as_deref() else {
                return ToolExecutionOutput::text(json_error("exec_cmd requires a chat session"));
            };
            {
                let (lock, _) = COMMAND_APPROVALS
                    .get_or_init(|| (Mutex::new(CommandApprovals::default()), Condvar::new()));
                lock.lock().unwrap().pending.insert(
                    session_id.to_string(),
                    PendingCommandApproval {
                        command: args.command.clone(),
                        shell: shell.clone(),
                    },
                );
            }
            (on_output)(
                "command_confirmation",
                &serde_json::json!({"command":args.command,"shell":shell}).to_string(),
            );
            if !wait_for_command_decision(session_id, &args.command) {
                return ToolExecutionOutput::text(serde_json::json!({"ok":false,"rejected":true,"error":"command execution rejected by user"}).to_string());
            }
        }
        let cwd = if let Some(raw) = args.cwd.as_deref() {
            match resolve_tool_path(self.workspace_path.as_deref(), raw) {
                Ok(v) => Some(v),
                Err(e) => return ToolExecutionOutput::text(json_error(e.to_string())),
            }
        } else {
            self.workspace_path.clone()
        };
        let secs = args.timeout_secs.unwrap_or(30);
        let max_output = args.max_output_bytes.unwrap_or(32 * 1024);
        if max_output == 0 {
            return ToolExecutionOutput::text(json_error(
                "max_output_bytes must be greater than zero",
            ));
        }
        let command_cwd = cwd.clone();
        let input = args.input.clone();
        let env = args.env.clone();
        let command_text = args.command.clone();
        let selected_shell = shell.clone();
        let result = block_async(async move {
            timeout(Duration::from_secs(secs), async move {
                let mut command = if selected_shell == "powershell" {
                    let mut c = Command::new("powershell");
                    c.args(["-NoProfile", "-Command", &command_text]);
                    c
                } else {
                    let mut c = Command::new("bash");
                    c.args(["-lc", &command_text]);
                    c
                };
                if let Some(path) = command_cwd.as_ref() {
                    command.current_dir(path);
                }
                command.envs(env);
                command
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());
                if input.is_some() {
                    command.stdin(std::process::Stdio::piped());
                } else {
                    command.stdin(std::process::Stdio::null());
                }
                let mut child = match command.spawn() {
                    Ok(child) => child,
                    Err(err) => return Err(err),
                };
                if let Some(input) = input {
                    use tokio::io::AsyncWriteExt;
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(input.as_bytes()).await;
                    }
                }
                let stdout = child.stdout.take().expect("exec_cmd stdout pipe must be configured");
                let stderr = child.stderr.take().expect("exec_cmd stderr pipe must be configured");
                let (output_tx, mut output_rx) =
                    mpsc::unbounded_channel::<(&'static str, Option<Vec<u8>>)>();
                tokio::spawn(forward_stream("stdout", stdout, output_tx.clone()));
                tokio::spawn(forward_stream("stderr", stderr, output_tx.clone()));
                drop(output_tx);
                let mut stdout_bytes = Vec::new();
                let mut stderr_bytes = Vec::new();
                let mut closed_streams = 0;
                let status = Box::pin(child.wait());
                while closed_streams < 2 {
                    if let Some((stream, chunk)) = output_rx.recv().await {
                        if let Some(chunk) = chunk {
                            if stream == "stdout" {
                                stdout_bytes.extend_from_slice(&chunk);
                            } else {
                                stderr_bytes.extend_from_slice(&chunk);
                            }
                            let text = String::from_utf8_lossy(&chunk);
                            if !text.is_empty() {
                                (on_output)(stream, &text);
                            }
                        } else {
                            closed_streams += 1;
                        }
                    } else {
                        break;
                    }
                }
                let process_status = status.await?;
                Ok(std::process::Output {
                    status: process_status,
                    stdout: stdout_bytes,
                    stderr: stderr_bytes,
                })
            })
            .await
        });
        let result = match result {
            Ok(Ok(output)) => {
                let (stdout, stdout_truncated) = truncate_output(&output.stdout, max_output);
                let (stderr, stderr_truncated) =
                    truncate_output(&output.stderr, max_output.saturating_sub(stdout.len()));
                let exit_code = output.status.code();
                success_json(
                    serde_json::json!({"ok":output.status.success(),"status":exit_code,"exit_code":exit_code,"stdout":stdout,"stderr":stderr,"output_truncated":stdout_truncated||stderr_truncated,"shell":shell,"cwd":cwd.as_ref().map(|p|p.display().to_string()),"error":if output.status.success(){Value::Null}else{Value::String(format!("command exited with status {}",exit_code.map_or_else(||"unknown".to_string(),|code|code.to_string())) )}}),
                )
            }
            Ok(Err(e)) => json_error(format!("failed to execute command: {e}")),
            Err(_) => json_error(format!("command timed out after {secs}s")),
        };
        ToolExecutionOutput::text(result)
    }
    fn execution_resource(&self, _: &Value) -> ToolExecutionResource {
        ToolExecutionResource::Exclusive
    }
    fn requires_user_confirmation(&self, a: &Value) -> bool {
        let args: ExecCmdArgs = match serde_json::from_value(a.clone()) {
            Ok(v) => v,
            Err(_) => return true,
        };
        !is_approved(self.session_id.as_deref(), &args.command)
    }
}

async fn forward_stream<R: AsyncRead + Unpin>(
    stream_name: &'static str,
    mut reader: R,
    output_tx: mpsc::UnboundedSender<(&'static str, Option<Vec<u8>>)>,
) {
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(size) => {
                if output_tx.send((stream_name, Some(buffer[..size].to_vec()))).is_err() {
                    return;
                }
            }
            Err(_) => break,
        }
    }
    let _ = output_tx.send((stream_name, None));
}
