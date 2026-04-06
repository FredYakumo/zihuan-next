use std::process::Command;

fn main() {
    // Re-run this build script only when web sources change
    println!("cargo:rerun-if-changed=webui/src");
    println!("cargo:rerun-if-changed=webui/index.html");
    println!("cargo:rerun-if-changed=webui/package.json");
    println!("cargo:rerun-if-changed=webui/vite.config.ts");
    println!("cargo:rerun-if-changed=webui/tsconfig.json");

    // On Windows pnpm is a .ps1/.cmd script, not a plain executable,
    // so we invoke it through cmd.exe.
    let status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/c", "pnpm", "run", "build"])
            .current_dir("webui")
            .status()
            .expect("Failed to run `cmd /c pnpm run build`. Is pnpm installed?")
    } else {
        Command::new("pnpm")
            .args(["run", "build"])
            .current_dir("webui")
            .status()
            .expect("Failed to run `pnpm run build`. Is pnpm installed?")
    };

    if !status.success() {
        panic!("`pnpm run build` failed with exit code: {:?}", status.code());
    }
}
