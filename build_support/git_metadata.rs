use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn find_repo_root(crate_dir: &Path) -> Option<&Path> {
    crate_dir
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
}

pub fn git_commit_id(repo_root: &Path) -> Option<String> {
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

fn resolve_git_dir(repo_root: &Path) -> Option<PathBuf> {
    let git_path = repo_root.join(".git");
    if git_path.is_dir() {
        return Some(git_path);
    }

    let git_dir = fs::read_to_string(&git_path).ok()?;
    let git_dir = git_dir.trim().strip_prefix("gitdir: ")?;
    let git_dir = Path::new(git_dir);

    Some(if git_dir.is_absolute() {
        git_dir.to_path_buf()
    } else {
        repo_root.join(git_dir)
    })
}

pub fn emit_git_rerun_hints(repo_root: &Path) {
    let Some(git_dir) = resolve_git_dir(repo_root) else {
        return;
    };
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
