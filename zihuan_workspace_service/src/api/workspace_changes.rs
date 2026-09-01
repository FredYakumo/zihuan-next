//! Workspace file-change tracking, persistence, and user-controlled rollback.
//!
//! This module implements the change-review layer for Workspace Agent file operations. The
//! underlying tools still write to disk immediately; this module records what happened after
//! the write so the dashboard can present a reviewable change without delaying the agent loop.
//!
//! mechanism:
//!
//! 1. `SseToolCallingObserver::on_tool_start` identifies a mutating workspace tool and calls
//!    [`WorkspaceChangeRecorder::start`]. The recorder resolves every affected path and captures
//!    its complete before-snapshot.
//! 2. The workspace tool executes normally and writes to disk.
//! 3. `SseToolCallingObserver::on_tool_finish` passes the tool result to
//!    [`WorkspaceChangeRecorder::finish`]. Failed tool results and no-op writes are ignored;
//!    successful writes are compared with a new after-snapshot.
//! 4. A [`WorkspaceChangeRecord`] is created, persisted, and emitted as a structured SSE event.
//!    The frontend uses that event to update the compact review panel and the detail dialog.
//! 5. Pending records whose affected paths overlap are folded into one logical record across chat
//!    rounds. The first before-snapshot for every path is retained, while the latest after-state,
//!    fingerprint, line counts, and diff replace the previous values. Accepting or canceling a
//!    record ends that merge window.
//! 6. Accept only changes the record state. Cancel first compares the current filesystem state
//!    with the recorded after-fingerprint; if another process changed the file, rollback is
//!    rejected to avoid overwriting external work. Otherwise the before-snapshot is restored.
//!
//! Metadata is stored per session as JSON, while full snapshots are stored in sidecar JSON files
//! keyed by change ID. This keeps the chat history format independent from binary file contents
//! and allows pending changes to be reconstructed after a page refresh or server restart.
//!
//! Copy and move operations are represented as one record containing both source and destination
//! paths. Directory snapshots recursively capture entries, so restoration can recreate files,
//! directories, and the original non-existent state.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use similar::{ChangeTag, TextDiff};
use uuid::Uuid;
use zihuan_core::error::{Error, Result};
use zihuan_core::system_config::app_data_dir;

const CHANGE_DIR_NAME: &str = "workspace_changes";

/// Lists the pending filesystem changes for a chat session.
///
/// This endpoint is used during session restore and page refresh. Accepted and canceled records
/// remain persisted for auditability but are filtered out by [`pending`].
#[handler]
pub async fn list_workspace_changes(req: &mut Request, res: &mut Response) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    match pending(&session_id) {
        Ok(changes) => res.render(Json(json!({ "changes": changes }))),
        Err(err) => render_internal_error(res, err),
    }
}

/// Accepts one change record while leaving the already-written filesystem untouched.
#[handler]
pub async fn accept_workspace_change(req: &mut Request, res: &mut Response) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    let change_id = req.param::<String>("change_id").unwrap_or_default();
    match accept(&session_id, &change_id) {
        Ok(change) => res.render(Json(json!({ "change": change }))),
        Err(err) => render_internal_error(res, err),
    }
}

/// Cancels one change record and restores its before-snapshot when conflict checks pass.
///
/// A filesystem fingerprint mismatch is returned as HTTP 409 so the frontend can keep the
/// record visible and explain that an external modification must be resolved manually.
#[handler]
pub async fn cancel_workspace_change(req: &mut Request, res: &mut Response) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    let change_id = req.param::<String>("change_id").unwrap_or_default();
    match cancel(&session_id, &change_id) {
        Ok(change) => res.render(Json(json!({ "change": change }))),
        Err(err) => {
            res.status_code(StatusCode::CONFLICT);
            res.render(Json(json!({ "error": err.to_string() })));
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceChangeOperation {
    /// A path did not exist before the operation and now contains a file or directory.
    Create,
    /// Existing file content was replaced in place.
    Edit,
    /// A file or directory was removed.
    Delete,
    /// A source was copied to a destination, possibly replacing the destination.
    Copy,
    /// A source was moved or renamed to a destination, possibly replacing the destination.
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceChangeStatus {
    /// The change is still visible in the dashboard review panel and can be handled by the user.
    Pending,
    /// The final filesystem state matches the original snapshot, so no review is needed unless a later write reopens it.
    Resolved,
    /// The user accepted the already-written filesystem state.
    Accepted,
    /// The user canceled the change and the before-snapshot was restored.
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDiffLine {
    /// Workspace-relative file path containing this line.
    #[serde(default)]
    pub path: String,
    /// `added`, `removed`, or unchanged `context`.
    pub kind: String,
    /// The source line captured from the before or after snapshot.
    pub line: String,
    /// One-based line number in the before snapshot, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_line: Option<usize>,
    /// One-based line number in the after snapshot, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_line: Option<usize>,
    /// Identifies a contiguous change block and its surrounding context.
    #[serde(default)]
    pub hunk: usize,
}
/// One serialized file or directory entry inside a path snapshot.
///
/// File bytes are hex encoded instead of interpreted as UTF-8, which makes rollback safe for
/// arbitrary file contents. Directory entries only need their relative path and directory flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotEntry {
    relative_path: String,
    is_directory: bool,
    content_hex: Option<String>,
}

/// Complete state of one affected path before or after a tool call.
///
/// The path itself is absolute for reliable restoration, while public records expose workspace-
/// relative paths for display. `exists` distinguishes a missing path from an empty directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PathSnapshot {
    path: String,
    exists: bool,
    entries: Vec<SnapshotEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceChangeRecord {
    /// Stable identifier used by SSE events and Accept/Cancel endpoints.
    pub change_id: String,
    /// Chat session that owns this review record.
    pub session_id: String,
    /// Workspace operation that produced the record.
    pub operation: WorkspaceChangeOperation,
    /// Workspace-relative display paths affected by the operation.
    pub paths: Vec<String>,
    /// Primary path shown in compact lists and as the file grouping key.
    pub display_path: String,
    /// Original source path for a copy or move operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Final destination path for a copy or move operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_path: Option<String>,
    /// Number of lines present only in the after-state summary.
    pub added_lines: usize,
    /// Number of lines present only in the before-state summary.
    pub removed_lines: usize,
    /// Fingerprint of all before snapshots, used for diagnostics and auditing.
    pub before_fingerprint: String,
    /// Fingerprint of all after snapshots, used to detect external changes before rollback.
    pub after_fingerprint: String,
    /// Current review state.
    pub status: WorkspaceChangeStatus,
    /// Number of successful workspace tool calls folded into this logical record.
    pub merged_count: usize,
    /// Coarse unified-style line list used by the dashboard detail dialog.
    #[serde(default)]
    pub diff: Vec<WorkspaceDiffLine>,
    /// Private recovery data retained in memory and in the change sidecar file.
    #[serde(skip)]
    before: Vec<PathSnapshot>,
    /// Private post-write state used for conflict detection.
    #[serde(skip)]
    after: Vec<PathSnapshot>,
}

/// Temporary correlation state held between observer start and finish callbacks.
#[derive(Debug, Clone)]
struct PendingOperation {
    session_id: String,
    operation: WorkspaceChangeOperation,
    paths: Vec<PathBuf>,
    source_path: Option<PathBuf>,
    destination_path: Option<PathBuf>,
    before: Vec<PathSnapshot>,
}

static RECORDS: OnceLock<Mutex<HashMap<String, Vec<WorkspaceChangeRecord>>>> = OnceLock::new();
static STARTED: OnceLock<Mutex<HashMap<String, PendingOperation>>> = OnceLock::new();

fn records() -> &'static Mutex<HashMap<String, Vec<WorkspaceChangeRecord>>> {
    RECORDS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn started() -> &'static Mutex<HashMap<String, PendingOperation>> {
    STARTED.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct WorkspaceChangeRecorder {
    pub session_id: String,
    workspace_path: Option<PathBuf>,
}

impl WorkspaceChangeRecorder {
    /// Creates a recorder for one chat session and loads any persisted records for that session.
    ///
    /// The recorder is request-scoped and shared with the ToolCallingEngine observer through an `Arc`. The
    /// session-level store is global so the REST handlers can access the same records later.
    pub fn new(session_id: impl Into<String>, workspace_path: Option<String>) -> Arc<Self> {
        let recorder = Arc::new(Self {
            session_id: session_id.into(),
            workspace_path: workspace_path.map(PathBuf::from),
        });
        let _ = load_session(&recorder.session_id);
        recorder
    }

    /// Captures the filesystem state before a mutating tool starts.
    ///
    /// The tool call ID is the temporary correlation key. It is removed when `finish` is called,
    /// so a failed or disconnected tool call cannot accidentally be reused by a later operation.
    pub fn start(&self, call_id: &str, operation: WorkspaceChangeOperation, arguments: &Value) {
        let paths = operation_paths(&operation, arguments)
            .into_iter()
            .filter_map(|path| resolve_path(self.workspace_path.as_deref(), &path))
            .collect::<Vec<_>>();
        let source_path =
            matches!(operation, WorkspaceChangeOperation::Copy | WorkspaceChangeOperation::Move)
                .then(|| paths.first().cloned())
                .flatten();
        let destination_path =
            matches!(operation, WorkspaceChangeOperation::Copy | WorkspaceChangeOperation::Move)
                .then(|| paths.get(1).cloned())
                .flatten();
        let before = paths.iter().map(|path| snapshot(path)).collect::<Vec<_>>();
        started().lock().expect("workspace change lock poisoned").insert(
            call_id.to_string(),
            PendingOperation {
                session_id: self.session_id.clone(),
                operation,
                paths,
                source_path,
                destination_path,
                before,
            },
        );
    }

    /// Finalizes a tool call after the tool has written to disk.
    ///
    /// A valid successful JSON result must contain `{"ok": true}`. The method then snapshots the
    /// affected paths again, ignores no-op changes, merges overlapping pending changes, persists
    /// the record and returns it for SSE emission. Persistence errors are intentionally
    /// best-effort here because the existing tool result must not be changed after the tool has
    /// completed.
    pub fn finish(&self, call_id: &str, result: &str) -> Option<WorkspaceChangeRecord> {
        let operation = started().lock().ok()?.remove(call_id)?;
        let result_json: Value = serde_json::from_str(result).ok()?;
        if result_json.get("ok").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        let after = operation.paths.iter().map(|path| snapshot(path)).collect::<Vec<_>>();
        if snapshots_equal(&operation.before, &after) {
            return None;
        }
        let record = WorkspaceChangeRecord {
            change_id: Uuid::new_v4().to_string(),
            session_id: operation.session_id.clone(),
            operation: operation.operation,
            paths: operation
                .paths
                .iter()
                .map(|path| display_path(self.workspace_path.as_deref(), path))
                .collect(),
            display_path: operation
                .paths
                .first()
                .map(|path| display_path(self.workspace_path.as_deref(), path))
                .unwrap_or_default(),
            source_path: operation
                .source_path
                .as_deref()
                .map(|path| display_path(self.workspace_path.as_deref(), path)),
            destination_path: operation
                .destination_path
                .as_deref()
                .map(|path| display_path(self.workspace_path.as_deref(), path)),
            added_lines: diff_stats(&operation.before, &after).0,
            removed_lines: diff_stats(&operation.before, &after).1,
            before_fingerprint: fingerprint(&operation.before),
            after_fingerprint: fingerprint(&after),
            status: WorkspaceChangeStatus::Pending,
            merged_count: 1,
            diff: build_diff(&operation.before, &after, self.workspace_path.as_deref()),
            before: operation.before,
            after,
        };

        let mut all = records().lock().expect("workspace change lock poisoned");
        let list = all.entry(self.session_id.clone()).or_default();
        let matching_indices = matching_pending_indices(list, &record.paths);
        if !matching_indices.is_empty() {
            return merge_pending_records(
                list,
                matching_indices,
                record,
                self.workspace_path.as_deref(),
                &self.session_id,
            );
        }
        persist_snapshot(&record).ok();
        list.push(record.clone());
        persist_session(&self.session_id, list).ok();
        Some(record)
    }
}

fn matching_pending_indices(records: &[WorkspaceChangeRecord], paths: &[String]) -> Vec<usize> {
    let paths = paths.iter().collect::<HashSet<_>>();
    records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            (matches!(
                record.status,
                WorkspaceChangeStatus::Pending | WorkspaceChangeStatus::Resolved
            ) && record.paths.iter().any(|path| paths.contains(path)))
            .then_some(index)
        })
        .collect()
}

fn merge_pending_records(
    records: &mut Vec<WorkspaceChangeRecord>,
    matching_indices: Vec<usize>,
    incoming: WorkspaceChangeRecord,
    workspace: Option<&Path>,
    session_id: &str,
) -> Option<WorkspaceChangeRecord> {
    let insertion_index = *matching_indices.first()?;
    let mut merged_records = matching_indices
        .iter()
        .rev()
        .map(|index| records.remove(*index))
        .collect::<Vec<_>>();
    merged_records.reverse();

    let change_id = merged_records.first()?.change_id.clone();
    let removed_change_ids = merged_records
        .iter()
        .skip(1)
        .map(|record| record.change_id.clone())
        .collect::<Vec<_>>();
    let before = merge_before_snapshots(&merged_records, &incoming.before);
    let after = before
        .iter()
        .map(|snapshot_item| snapshot(Path::new(&snapshot_item.path)))
        .collect::<Vec<_>>();
    let (operation, source_path, destination_path) =
        merged_operation(&merged_records, &incoming, &before, &after);
    let paths = before
        .iter()
        .map(|snapshot_item| display_path(workspace, Path::new(&snapshot_item.path)))
        .collect::<Vec<_>>();
    let display_path = source_path
        .clone()
        .unwrap_or_else(|| paths.first().cloned().unwrap_or_default());
    let mut merged = WorkspaceChangeRecord {
        change_id,
        session_id: incoming.session_id.clone(),
        operation,
        paths,
        display_path,
        source_path,
        destination_path,
        added_lines: diff_stats(&before, &after).0,
        removed_lines: diff_stats(&before, &after).1,
        before_fingerprint: fingerprint(&before),
        after_fingerprint: fingerprint(&after),
        status: WorkspaceChangeStatus::Pending,
        merged_count: merged_records.iter().map(|record| record.merged_count).sum::<usize>()
            + incoming.merged_count,
        diff: build_diff(&before, &after, workspace),
        before,
        after,
    };

    if snapshots_equal(&merged.before, &merged.after) {
        merged.status = WorkspaceChangeStatus::Resolved;
    }

    persist_snapshot(&merged).ok();
    for change_id in removed_change_ids {
        remove_snapshot(&change_id);
    }
    records.insert(insertion_index, merged.clone());
    persist_session(session_id, records).ok();
    Some(merged)
}

fn merge_before_snapshots(
    records: &[WorkspaceChangeRecord],
    incoming: &[PathSnapshot],
) -> Vec<PathSnapshot> {
    let mut snapshots = BTreeMap::new();
    for record in records {
        for snapshot_item in &record.before {
            snapshots
                .entry(snapshot_item.path.clone())
                .or_insert_with(|| snapshot_item.clone());
        }
    }
    for snapshot_item in incoming {
        snapshots
            .entry(snapshot_item.path.clone())
            .or_insert_with(|| snapshot_item.clone());
    }
    snapshots.into_values().collect()
}

fn merged_operation(
    records: &[WorkspaceChangeRecord],
    incoming: &WorkspaceChangeRecord,
    before: &[PathSnapshot],
    after: &[PathSnapshot],
) -> (WorkspaceChangeOperation, Option<String>, Option<String>) {
    if matches!(incoming.operation, WorkspaceChangeOperation::Copy) {
        return (
            WorkspaceChangeOperation::Copy,
            incoming.source_path.clone(),
            incoming.destination_path.clone(),
        );
    }
    if matches!(incoming.operation, WorkspaceChangeOperation::Move) {
        let source_path = records
            .iter()
            .rev()
            .find(|record| {
                matches!(record.operation, WorkspaceChangeOperation::Move)
                    && record.destination_path == incoming.source_path
            })
            .and_then(|record| record.source_path.clone())
            .or_else(|| incoming.source_path.clone());
        return (WorkspaceChangeOperation::Move, source_path, incoming.destination_path.clone());
    }
    if let Some(record) = records.iter().rev().find(|record| {
        matches!(
            record.operation,
            WorkspaceChangeOperation::Copy | WorkspaceChangeOperation::Move
        )
    }) {
        return (
            record.operation.clone(),
            record.source_path.clone(),
            record.destination_path.clone(),
        );
    }
    (derived_operation(before, after), None, None)
}

fn derived_operation(before: &[PathSnapshot], after: &[PathSnapshot]) -> WorkspaceChangeOperation {
    let existed_before = before.iter().any(|snapshot_item| snapshot_item.exists);
    let exists_after = after.iter().any(|snapshot_item| snapshot_item.exists);
    match (existed_before, exists_after) {
        (false, true) => WorkspaceChangeOperation::Create,
        (true, false) => WorkspaceChangeOperation::Delete,
        _ => WorkspaceChangeOperation::Edit,
    }
}

/// Returns all pending changes belonging to a chat session.
///
/// Loading is lazy: the first call reconstructs record metadata and private snapshots from the
/// session JSON and change sidecars. The frontend uses this endpoint when opening or refreshing
/// a conversation.
pub fn pending(session_id: &str) -> Result<Vec<WorkspaceChangeRecord>> {
    load_session(session_id)?;
    let all = records()
        .lock()
        .map_err(|_| zihuan_core::string_error!("workspace change lock poisoned"))?;
    Ok(all
        .get(session_id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|item| matches!(item.status, WorkspaceChangeStatus::Pending))
        .collect())
}

/// Marks one pending change as accepted without touching the filesystem.
pub fn accept(session_id: &str, change_id: &str) -> Result<WorkspaceChangeRecord> {
    update_status(session_id, change_id, WorkspaceChangeStatus::Accepted, false)
}

/// Attempts to cancel one pending change and restore its before-snapshot.
///
/// Rollback is guarded by the after-fingerprint. A mismatch means another operation changed one
/// of the affected paths after the agent write, so the method returns a conflict error instead of
/// overwriting that newer state.
pub fn cancel(session_id: &str, change_id: &str) -> Result<WorkspaceChangeRecord> {
    load_session(session_id)?;
    let mut all = records()
        .lock()
        .map_err(|_| zihuan_core::string_error!("workspace change lock poisoned"))?;
    let record = all
        .get_mut(session_id)
        .and_then(|items| items.iter_mut().find(|item| item.change_id == change_id))
        .ok_or_else(|| Error::ValidationError("workspace change not found".to_string()))?;
    if !matches!(record.status, WorkspaceChangeStatus::Pending) {
        return Ok(record.clone());
    }
    let current = record
        .after
        .iter()
        .map(|item| snapshot(Path::new(&item.path)))
        .collect::<Vec<_>>();
    if fingerprint(&current) != record.after_fingerprint {
        return Err(Error::ValidationError("文件已被其他操作修改，无法自动回滚".to_string()));
    }
    for snapshot_item in &record.before {
        restore(snapshot_item)?;
    }
    record.status = WorkspaceChangeStatus::Canceled;
    let output = record.clone();
    let items = all.get(session_id).unwrap();
    persist_session(session_id, items)?;
    Ok(output)
}

fn update_status(
    session_id: &str,
    change_id: &str,
    status: WorkspaceChangeStatus,
    _write: bool,
) -> Result<WorkspaceChangeRecord> {
    load_session(session_id)?;
    let mut all = records()
        .lock()
        .map_err(|_| zihuan_core::string_error!("workspace change lock poisoned"))?;
    let items = all
        .get_mut(session_id)
        .ok_or_else(|| Error::ValidationError("workspace change not found".to_string()))?;
    let record = items
        .iter_mut()
        .find(|item| item.change_id == change_id)
        .ok_or_else(|| Error::ValidationError("workspace change not found".to_string()))?;
    if matches!(record.status, WorkspaceChangeStatus::Pending) {
        record.status = status;
    }
    let output = record.clone();
    persist_session(session_id, items)?;
    Ok(output)
}

fn operation_paths(operation: &WorkspaceChangeOperation, args: &Value) -> Vec<String> {
    match operation {
        WorkspaceChangeOperation::Edit => args
            .get("patch")
            .and_then(Value::as_str)
            .map(|patch| {
                patch
                    .lines()
                    .filter_map(|line| line.strip_prefix("*** Update File: "))
                    .map(|path| path.trim().to_string())
                    .collect()
            })
            .or_else(|| args.get("path").and_then(Value::as_str).map(|path| vec![path.to_string()]))
            .unwrap_or_default(),
        WorkspaceChangeOperation::Create | WorkspaceChangeOperation::Delete => args
            .get("path")
            .and_then(Value::as_str)
            .map(|v| vec![v.to_string()])
            .unwrap_or_default(),
        WorkspaceChangeOperation::Copy | WorkspaceChangeOperation::Move => [
            args.get("src").and_then(Value::as_str),
            args.get("dest").and_then(Value::as_str),
        ]
        .into_iter()
        .flatten()
        .map(ToOwned::to_owned)
        .collect(),
    }
}

/// Maps a ToolCallingEngine tool name to the set of operations that can create review records.
///
/// `exec_cmd` is deliberately excluded because arbitrary command side effects cannot be safely
/// reconstructed from its output.
pub fn operation_for_tool(name: &str) -> Option<WorkspaceChangeOperation> {
    match name {
        "create_file" => Some(WorkspaceChangeOperation::Create),
        "edit_file" => Some(WorkspaceChangeOperation::Edit),
        "delete_file" => Some(WorkspaceChangeOperation::Delete),
        "copy_file" => Some(WorkspaceChangeOperation::Copy),
        "move_file" => Some(WorkspaceChangeOperation::Move),
        _ => None,
    }
}

fn resolve_path(workspace: Option<&Path>, raw: &str) -> Option<PathBuf> {
    let path = PathBuf::from(raw);
    Some(if path.is_absolute() {
        path
    } else {
        workspace?.join(path)
    })
}
fn display_path(workspace: Option<&Path>, path: &Path) -> String {
    workspace
        .and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Captures a path as a deterministic, serializable snapshot.
///
/// Files are stored as hex-encoded bytes so binary data and platform-specific line endings survive
/// a rollback unchanged. Directories are represented as recursive relative entries. A missing
/// path has `exists == false` and an empty entry list, which lets restoration remove a newly
/// created path rather than writing an empty placeholder.
fn snapshot(path: &Path) -> PathSnapshot {
    let mut entries = Vec::new();
    if path.is_dir() {
        collect_entries(path, path, &mut entries);
    } else if path.is_file() {
        entries.push(SnapshotEntry {
            relative_path: String::new(),
            is_directory: false,
            content_hex: fs::read(path).ok().map(|bytes| hex_encode(&bytes)),
        });
    }
    PathSnapshot {
        path: path.to_string_lossy().to_string(),
        exists: path.exists(),
        entries,
    }
}

/// Recursively appends directory entries to a path snapshot.
fn collect_entries(root: &Path, current: &Path, entries: &mut Vec<SnapshotEntry>) {
    let Ok(read_dir) = fs::read_dir(current) else {
        return;
    };
    let mut paths = read_dir.flatten().map(|item| item.path()).collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let relative_path = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
        if path.is_dir() {
            entries.push(SnapshotEntry {
                relative_path: relative_path.clone(),
                is_directory: true,
                content_hex: None,
            });
            collect_entries(root, &path, entries);
        } else if path.is_file() {
            entries.push(SnapshotEntry {
                relative_path,
                is_directory: false,
                content_hex: fs::read(&path).ok().map(|bytes| hex_encode(&bytes)),
            });
        }
    }
}
fn snapshots_equal(before: &[PathSnapshot], after: &[PathSnapshot]) -> bool {
    fingerprint(before) == fingerprint(after)
}

/// Produces the stable fingerprint used to compare complete multi-path states.
fn fingerprint(items: &[PathSnapshot]) -> String {
    let json = serde_json::to_vec(items).unwrap_or_default();
    hex_encode(&json)
}
fn diff_stats(before: &[PathSnapshot], after: &[PathSnapshot]) -> (usize, usize) {
    build_diff(before, after, None)
        .into_iter()
        .fold((0, 0), |(added, removed), line| match line.kind.as_str() {
            "added" => (added + 1, removed),
            "removed" => (added, removed + 1),
            _ => (added, removed),
        })
}

fn build_diff(
    before: &[PathSnapshot],
    after: &[PathSnapshot],
    workspace: Option<&Path>,
) -> Vec<WorkspaceDiffLine> {
    const CONTEXT_LINES: usize = 10;
    let mut lines = Vec::new();
    let before_files = snapshot_files(before);
    let after_files = snapshot_files(after);
    let paths = before_files.keys().chain(after_files.keys()).collect::<BTreeSet<_>>();

    for (file_index, path) in paths.into_iter().enumerate() {
        let old = before_files.get(path).map(String::as_str).unwrap_or_default();
        let new = after_files.get(path).map(String::as_str).unwrap_or_default();
        let diff = TextDiff::from_lines(old, new);
        let display_path = display_path(workspace, Path::new(path));
        for (hunk, operations) in diff.grouped_ops(CONTEXT_LINES).into_iter().enumerate() {
            for operation in operations {
                for change in diff.iter_changes(&operation) {
                    let (kind, before_line, after_line) = match change.tag() {
                        ChangeTag::Delete => {
                            ("removed", change.old_index().map(|index| index + 1), None)
                        }
                        ChangeTag::Insert => {
                            ("added", None, change.new_index().map(|index| index + 1))
                        }
                        ChangeTag::Equal => (
                            "context",
                            change.old_index().map(|index| index + 1),
                            change.new_index().map(|index| index + 1),
                        ),
                    };

                    lines.push(WorkspaceDiffLine {
                        path: display_path.clone(),
                        kind: kind.to_string(),
                        line: change.value().trim_end_matches(['\r', '\n']).to_string(),
                        before_line,
                        after_line,
                        hunk: file_index * 1_000_000 + hunk,
                    });
                }
            }
        }
    }

    lines
}
fn snapshot_files(items: &[PathSnapshot]) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for item in items {
        for entry in &item.entries {
            let Some(content_hex) = &entry.content_hex else {
                continue;
            };
            let path = if entry.relative_path.is_empty() {
                item.path.clone()
            } else {
                Path::new(&item.path).join(&entry.relative_path).to_string_lossy().to_string()
            };
            files.insert(path, String::from_utf8_lossy(&hex_decode(content_hex)).to_string());
        }
    }
    files
}
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn hex_decode(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks(2)
        .filter_map(|chunk| u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok())
        .collect()
}

fn restore(snapshot: &PathSnapshot) -> Result<()> {
    let path = PathBuf::from(&snapshot.path);
    if path.exists() {
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    if !snapshot.exists {
        return Ok(());
    }
    if snapshot.entries.len() == 1 && snapshot.entries[0].relative_path.is_empty() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(
            path,
            hex_decode(snapshot.entries[0].content_hex.as_deref().unwrap_or_default()),
        )?;
        return Ok(());
    }
    fs::create_dir_all(&path)?;
    for entry in &snapshot.entries {
        let target = path.join(&entry.relative_path);
        if entry.is_directory {
            fs::create_dir_all(target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(target, hex_decode(entry.content_hex.as_deref().unwrap_or_default()))?;
        }
    }
    Ok(())
}

/// Returns the application storage location for change metadata and snapshot sidecars.
fn storage_dir() -> PathBuf {
    app_data_dir().join(CHANGE_DIR_NAME)
}

/// Returns the per-session metadata file path.
fn session_file(session_id: &str) -> PathBuf {
    storage_dir().join(format!("{session_id}.json"))
}

/// Stores private before/after snapshots separately from the public record metadata.
fn persist_snapshot(record: &WorkspaceChangeRecord) -> Result<()> {
    let path = storage_dir().join(format!("{}-snapshot.json", record.change_id));
    fs::create_dir_all(storage_dir())?;
    let data = serde_json::to_vec(&(record.before.clone(), record.after.clone()))
        .map_err(|e| zihuan_core::string_error!("{}", e))?;
    fs::write(path, data)?;
    Ok(())
}

fn remove_snapshot(change_id: &str) {
    let path = storage_dir().join(format!("{change_id}-snapshot.json"));
    let _ = fs::remove_file(path);
}

/// Writes the session's complete record list so status changes and merge results survive restart.
fn persist_session(session_id: &str, items: &[WorkspaceChangeRecord]) -> Result<()> {
    fs::create_dir_all(storage_dir())?;
    let data = serde_json::to_vec_pretty(items).map_err(|e| zihuan_core::string_error!("{}", e))?;
    fs::write(session_file(session_id), data)?;
    Ok(())
}

/// Lazily reconstructs a session from its metadata file and snapshot sidecars.
fn load_session(session_id: &str) -> Result<()> {
    let mut all = records()
        .lock()
        .map_err(|_| zihuan_core::string_error!("workspace change lock poisoned"))?;
    if all.contains_key(session_id) {
        return Ok(());
    }
    let path = session_file(session_id);
    if !path.exists() {
        all.insert(session_id.to_string(), Vec::new());
        return Ok(());
    }
    let mut items: Vec<WorkspaceChangeRecord> = serde_json::from_slice(&fs::read(path)?)
        .map_err(|e| zihuan_core::string_error!("{}", e))?;
    for item in &mut items {
        let snapshot_path = storage_dir().join(format!("{}-snapshot.json", item.change_id));
        if snapshot_path.exists() {
            if let Ok((before, after)) = serde_json::from_slice(&fs::read(snapshot_path)?) {
                item.before = before;
                item.after = after;
            }
        }
    }
    all.insert(session_id.to_string(), items);
    Ok(())
}

fn render_internal_error(res: &mut Response, err: impl ToString) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(Json(json!({ "error": err.to_string() })));
}
