use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let crate_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let repo_root = crate_dir
        .parent()
        .and_then(Path::parent)
        .expect("model_inference crate should live under workspace/packages");

    emit_git_rerun_hints(repo_root);

    let commit_id = git_commit_id(repo_root).unwrap_or_else(|| "unknown".to_string());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("missing OUT_DIR"));
    let metadata_rs = out_dir.join("build_metadata.rs");
    let metadata_source = format!("pub const ZIHUAN_GIT_COMMIT_ID: &str = {:?};\n", commit_id);

    fs::write(&metadata_rs, metadata_source).expect("failed to write build metadata");
}

fn git_commit_id(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let commit_id = String::from_utf8(output.stdout).ok()?;
    let trimmed = commit_id.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn emit_git_rerun_hints(repo_root: &Path) {
    let git_dir = repo_root.join(".git");
    let head_path = git_dir.join("HEAD");

    println!("cargo:rerun-if-changed={}", head_path.display());

    let Ok(head) = fs::read_to_string(&head_path) else {
        return;
    };
    let Some(reference) = head.trim().strip_prefix("ref: ") else {
        return;
    };

    let ref_path = git_dir.join(reference);
    println!("cargo:rerun-if-changed={}", ref_path.display());
}
