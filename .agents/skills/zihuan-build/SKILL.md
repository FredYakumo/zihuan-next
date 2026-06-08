---
name: zihuan-build
description: Build the zihuan-next project from source. Use this skill when asked to build, compile, or run the Rust project, including CUDA/Metal feature builds, frontend builds, and Docker builds.
---

# Building zihuan-next

The project uses **Cargo** as its Rust build system, with `build.rs` automatically triggering the frontend (`webui/`) build via **pnpm**.

## Quick start

```bash
# Debug build (default)
cargo build

# Release build (recommended for production)
cargo build --release

# Build and run the server
cargo run

# Build and run with custom host/port
cargo run -- --host 0.0.0.0 --port 8080
```

## Build phases

The `build.rs` script runs **before** every Rust compilation and handles the frontend build automatically:

1. **Frontend build** — `pnpm install --frozen-lockfile` then `pnpm run build` in `webui/`
2. **HTML sanitization** — strips `crossorigin` attributes from the built `index.html`
3. **Embedding** — the result is embedded into the Rust binary via `rust-embed`

If the frontend build fails, the entire Rust build fails. Check `webui/dist/index.html` exists after a successful build.

## Feature flags

| Feature | Description |
|---------|-------------|
| `candle-cuda` | Enable CUDA GPU acceleration for model inference (NVIDIA GPUs) |
| `candle-metal` | Enable Metal GPU acceleration for model inference (macOS) |

```bash
# Build with CUDA support
cargo build --features candle-cuda --release

# Build with Metal support (macOS only)
cargo build --features candle-metal --release
```

### CUDA build on Windows

Use the helper script `scripts/cargo-cuda.ps1` which automatically locates the MSVC compiler and injects the `candle-cuda` feature:

```powershell
# Debug CUDA build
.\scripts\cargo-cuda.ps1

# Release CUDA build
.\scripts\cargo-cuda.ps1 -Release
```

## Frontend-only build

If you only need to iterate on the frontend without rebuilding Rust:

```bash
cd webui
pnpm install
pnpm run dev      # HMR dev server (default port varies)
pnpm run build    # production build to webui/dist/
```

## Docker build

The `docker/Dockerfile` uses a multi-stage build:

```bash
# Build the full Docker image (frontend + Rust backend)
docker build -f docker/Dockerfile -t zihuan-next .

# Start dependent services (Redis, rustfs)
docker compose -f docker/docker-compose.yaml up -d
```

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ZIHUAN_HOST` | `127.0.0.1` | Server bind address |
| `ZIHUAN_PORT` | `9951` | Server port |

## Build output

- `target/debug/zihuan_next(.exe)` — debug binary
- `target/release/zihuan_next(.exe)` — release binary
- `webui/dist/` — built frontend assets (embedded into binary)

## Agent tips

- **First build is slow** — expect several minutes for dependency compilation. Subsequent builds leverage incremental compilation.
- **If the frontend build fails**, try running `pnpm install` manually in `webui/` first to ensure all npm dependencies are installed.
- **Use `--release`** for any build you intend to run in production or measure performance on.
- **Redirect build output to a file** for large builds: `cargo build --release > build.log 2>&1`.
- **Run builds in the background** for full release builds — they can take 10+ minutes on first compile.
- **The `build.rs` re-runs** whenever files in `webui/src/`, `webui/index.html`, `webui/package.json`, `webui/pnpm-lock.yaml`, `webui/vite.config.ts`, or `webui/tsconfig.json` change.
