use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[path = "../build_support/git_metadata.rs"]
mod git_metadata;

fn main() {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let repo_root = git_metadata::find_repo_root(&crate_dir).expect("failed to locate repository root from crate dir");

    git_metadata::emit_git_rerun_hints(repo_root);

    let commit_id = git_metadata::git_commit_id(repo_root).unwrap_or_else(|| "unknown".to_string());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("missing OUT_DIR"));
    let metadata_rs = out_dir.join("build_metadata.rs");
    let metadata_source = format!("pub const ZIHUAN_GIT_COMMIT_ID: &str = {:?};\n", commit_id);

    fs::write(&metadata_rs, metadata_source).expect("failed to write build metadata");

    for path in [
        "webui/src",
        "webui/index.html",
        "webui/package.json",
        "webui/pnpm-lock.yaml",
        "webui/vite.config.ts",
        "webui/tsconfig.json",
    ] {
        println!("cargo:rerun-if-changed={}", repo_root.join(path).display());
    }

    if std::env::var_os("ZIHUAN_SKIP_FRONTEND_BUILD").is_none() {
        let webui_dir = repo_root.join("webui");
        let status = run_frontend_build(&webui_dir);
        if !status.success() {
            panic!("`pnpm run build` failed with exit code: {:?}", status.code());
        }

        let dist_index = webui_dir.join("dist/index.html");
        let html = fs::read_to_string(&dist_index)
            .expect("failed to read generated webui/dist/index.html after frontend build");
        let sanitized = html.replace(" crossorigin", "");
        if sanitized != html {
            fs::write(dist_index, sanitized).expect("failed to sanitize generated webui/dist/index.html");
        }
    }
}

#[cfg(target_os = "windows")]
fn run_frontend_build(webui_dir: &std::path::Path) -> std::process::ExitStatus {
    for args in [
        vec!["/c", "pnpm", "run", "build"],
        vec!["/c", "corepack", "pnpm", "run", "build"],
        vec!["/c", "npm", "exec", "--", "pnpm", "run", "build"],
    ] {
        match Command::new("cmd").args(args).current_dir(webui_dir).status() {
            Ok(status) if status.success() => return status,
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("failed to run frontend build command: {error}"),
        }
    }
    panic!("failed to run frontend build; install pnpm, corepack, or npm");
}

#[cfg(not(target_os = "windows"))]
fn run_frontend_build(webui_dir: &std::path::Path) -> std::process::ExitStatus {
    for (program, args) in [
        ("pnpm", vec!["run", "build"]),
        ("corepack", vec!["pnpm", "run", "build"]),
        ("npm", vec!["exec", "--", "pnpm", "run", "build"]),
    ] {
        match Command::new(program).args(args).current_dir(webui_dir).status() {
            Ok(status) if status.success() => return status,
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("failed to run frontend build command `{program}`: {error}"),
        }
    }
    panic!("failed to run frontend build; install pnpm, corepack, or npm");
}
