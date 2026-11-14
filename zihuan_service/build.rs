use std::env;
use std::path::PathBuf;

/// Shared build-script helpers (repository discovery and git metadata).
#[allow(dead_code)]
mod git_metadata {
    include!("../build_support/git_metadata.rs");
}

/// Resolves the version stamped into the binary.
///
/// CI builds a tagged commit, where the tag name is authoritative. Local builds
/// fall back to `git describe` so `zihuan_next --version` and the admin UI report
/// the same string the release archive is named after.
fn resolve_version(repo_root: Option<&std::path::Path>) -> String {
    if let (Ok(reference_type), Ok(reference_name)) =
        (env::var("GITHUB_REF_TYPE"), env::var("GITHUB_REF_NAME"))
    {
        if reference_type == "tag" && !reference_name.trim().is_empty() {
            return reference_name;
        }
    }

    repo_root
        .and_then(git_metadata::git_describe)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned())
}

/// Describes the artifact as `<profile>-<accelerator>`, for example
/// `release-metal`, matching the `cpu`/`cuda`/`metal` release variants.
fn resolve_build_type() -> String {
    let profile = env::var("PROFILE").unwrap_or_else(|_| "release".to_string());
    let accelerator = if env::var_os("CARGO_FEATURE_CANDLE_CUDA").is_some() {
        "cuda"
    } else if env::var_os("CARGO_FEATURE_CANDLE_METAL").is_some() {
        "metal"
    } else {
        "cpu"
    };

    format!("{profile}-{accelerator}")
}

fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_TYPE");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = git_metadata::find_repo_root(&manifest_dir).map(std::path::Path::to_path_buf);
    if let Some(repo_root) = repo_root.as_deref() {
        git_metadata::emit_git_rerun_hints(repo_root);
    }

    println!(
        "cargo:rustc-env=ZIHUAN_BUILD_VERSION={}",
        resolve_version(repo_root.as_deref())
    );
    println!(
        "cargo:rustc-env=ZIHUAN_BUILD_TYPE={}",
        resolve_build_type()
    );
}
