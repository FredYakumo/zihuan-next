---
name: zihuan-build
description: Build or run ZiHuan Next. Use when compiling, starting the Rust service, building the WebUI, working with CUDA or Metal features, or using Docker.
---

# Build And Run

```powershell
cargo build
cargo build --release
cargo run
```

- The workspace defaults to `zihuan_service` and `zihuan_cli`; use `cargo build -p <crate>` for narrow work.
- Build the frontend independently from `webui/` with `pnpm install --frozen-lockfile` and `pnpm run build`; use `pnpm run dev` for HMR.
- Use `cargo build --features candle-cuda --release` for CUDA, or `scripts/cargo-cuda.ps1 -Release` on Windows when applicable. Use `candle-metal` only on macOS.
- Build the container with `docker build -f docker/Dockerfile -t zihuan-next .`; inspect the active compose files under `docker/` before starting dependencies.
