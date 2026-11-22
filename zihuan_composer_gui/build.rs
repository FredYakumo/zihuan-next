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
    let status = std::process::Command::new(program)
        .args(["--dir", "../webui", "run", "build:composer"])
        .status()
        .expect("Composer GUI requires pnpm and installed webui dependencies");
    assert!(status.success(), "Composer frontend build failed");
    tauri_build::build();
}
