use std::fs;
use std::path::PathBuf;

#[path = "../build_support/git_metadata.rs"]
mod git_metadata;

fn main() {
    let crate_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let repo_root = git_metadata::find_repo_root(&crate_dir)
        .expect("failed to locate repository root from crate dir");

    git_metadata::emit_git_rerun_hints(repo_root);

    let commit_id = git_metadata::git_commit_id(repo_root).unwrap_or_else(|| "unknown".to_string());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("missing OUT_DIR"));
    let metadata_rs = out_dir.join("build_metadata.rs");
    let metadata_source = format!("pub const ZIHUAN_GIT_COMMIT_ID: &str = {:?};\n", commit_id);

    fs::write(&metadata_rs, metadata_source).expect("failed to write build metadata");
}
