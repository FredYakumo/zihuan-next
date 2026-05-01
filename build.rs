use std::{fs, path::Path, process::Command};

fn main() {
    // Re-run this build script only when web sources change
    println!("cargo:rerun-if-changed=webui/src");
    println!("cargo:rerun-if-changed=webui/index.html");
    println!("cargo:rerun-if-changed=webui/package.json");
    println!("cargo:rerun-if-changed=webui/pnpm-lock.yaml");
    println!("cargo:rerun-if-changed=webui/vite.config.ts");
    println!("cargo:rerun-if-changed=webui/tsconfig.json");

    // On Windows pnpm is a .ps1/.cmd script, not a plain executable,
    // so invoke it through cmd.exe.
    #[cfg(target_os = "windows")]
    let status = run_frontend_build_windows();

    #[cfg(not(target_os = "windows"))]
    let status = run_frontend_build_unix();

    if !status.success() {
        panic!(
            "`pnpm run build` failed with exit code: {:?}",
            status.code()
        );
    }

    let dist_index = Path::new("webui/dist/index.html");
    let html = fs::read_to_string(dist_index)
        .expect("Failed to read generated webui/dist/index.html after frontend build");
    let sanitized = html.replace(" crossorigin", "");
    if sanitized != html {
        fs::write(dist_index, sanitized)
            .expect("Failed to sanitize generated webui/dist/index.html");
    }
}

#[cfg(target_os = "windows")]
fn run_frontend_build_windows() -> std::process::ExitStatus {
    for args in [
        vec!["/c", "pnpm", "run", "build"],
        vec!["/c", "corepack", "pnpm", "run", "build"],
        vec!["/c", "npm", "exec", "--", "pnpm", "run", "build"],
    ] {
        match Command::new("cmd").args(args).current_dir("webui").status() {
            Ok(status) => return status,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => panic!("Failed to run frontend build command: {err}"),
        }
    }
    panic!("Failed to run frontend build. Install `pnpm`, `corepack`, or `npm`.");
}

#[cfg(not(target_os = "windows"))]
fn run_frontend_build_unix() -> std::process::ExitStatus {
    for (program, args) in [
        ("pnpm", vec!["run", "build"]),
        ("corepack", vec!["pnpm", "run", "build"]),
        ("npm", vec!["exec", "--", "pnpm", "run", "build"]),
    ] {
        match Command::new(program)
            .args(args)
            .current_dir("webui")
            .status()
        {
            Ok(status) => return status,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => panic!("Failed to run frontend build command `{program}`: {err}"),
        }
    }
    panic!("Failed to run frontend build. Install `pnpm`, `corepack`, or `npm`.");
}
