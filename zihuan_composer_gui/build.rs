fn main() {
    for path in [
        "../webui/composer",
        "../webui/vite.composer.config.ts",
        "../webui/package.json",
        "../webui/pnpm-lock.yaml",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let program = if cfg!(windows) { "pnpm.cmd" } else { "pnpm" };
    // Run from inside the webui project: corepack resolves the pnpm version
    // from the nearest package.json by CWD (it ignores `--dir`), so from this
    // crate's directory it would pick a default pnpm that does not match
    // webui's "packageManager" pin and refuse to build.
    let status = std::process::Command::new(program)
        .current_dir("../webui")
        .args(["run", "build:composer"])
        .status()
        .expect("Composer GUI requires pnpm and installed webui dependencies");
    assert!(status.success(), "Composer frontend build failed");
    tauri_build::build();
}
