#[allow(dead_code)]
mod git_metadata {
    include!("../build_support/git_metadata.rs");
}

fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_TYPE");
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_root = git_metadata::find_repo_root(&manifest_dir);
    if let Some(root) = repo_root {
        git_metadata::emit_git_rerun_hints(root);
    }
    let version = match (std::env::var("GITHUB_REF_TYPE"), std::env::var("GITHUB_REF_NAME")) {
        (Ok(kind), Ok(name)) if kind == "tag" && !name.trim().is_empty() => name,
        _ => repo_root
            .and_then(git_metadata::git_describe)
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()),
    };
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "release".to_owned());
    println!("cargo:rustc-env=ZIHUAN_BUILD_VERSION={version}");
    println!("cargo:rustc-env=ZIHUAN_BUILD_TYPE={profile}-cpu");
}
