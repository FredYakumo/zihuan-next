# ---- Build frontend ----
FROM node:22-slim AS frontend-builder

RUN corepack enable && corepack prepare pnpm@10.22.0 --activate

WORKDIR /build/webui
COPY webui/package.json webui/pnpm-lock.yaml webui/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY webui/ ./
RUN pnpm run build

# ---- Rust backend ----
FROM rust:1.83-bookworm AS backend-builder

RUN apt-get update && apt-get install -y \
    g++ \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependency builds by copying manifests first
COPY Cargo.toml Cargo.lock ./
COPY node_macros/Cargo.toml node_macros/Cargo.toml
COPY general_wheel_cpp/Cargo.toml general_wheel_cpp/Cargo.toml
COPY dynamic_script_engine/Cargo.toml dynamic_script_engine/Cargo.toml
COPY zihuan_core/Cargo.toml zihuan_core/Cargo.toml
COPY zihuan_service/Cargo.toml zihuan_service/Cargo.toml
COPY zihuan_workspace_service/Cargo.toml zihuan_workspace_service/Cargo.toml
COPY zihuan_ims_service/Cargo.toml zihuan_ims_service/Cargo.toml
COPY zihuan_cli/Cargo.toml zihuan_cli/Cargo.toml

# Create stub source files so cargo can resolve the workspace
RUN mkdir -p node_macros/src general_wheel_cpp/src dynamic_script_engine/src zihuan_core/src zihuan_service/src \
    zihuan_workspace_service/src zihuan_ims_service/src zihuan_cli/src && \
    echo "fn main() {}" > node_macros/src/lib.rs && \
    echo "fn main() {}" > general_wheel_cpp/src/lib.rs && \
    echo "fn main() {}" > dynamic_script_engine/src/lib.rs && \
    echo "fn main() {}" > zihuan_core/src/lib.rs && \
    echo "fn main() {}" > zihuan_service/src/lib.rs && \
    echo "fn main() {}" > zihuan_workspace_service/src/lib.rs && \
    echo "fn main() {}" > zihuan_ims_service/src/lib.rs && \
    echo "fn main() {}" > zihuan_cli/src/main.rs

# Patch crate for third_party
COPY third_party/ third_party/

RUN cargo build --release -p zihuan_service --bin zihuan_next 2>/dev/null || true

# copy actual source and C++ native code, then do the real build
COPY build_support/ build_support/
COPY general_wheel_cpp/build.rs general_wheel_cpp/build.rs
COPY general_wheel_cpp/native/ general_wheel_cpp/native/
COPY general_wheel_cpp/src/ general_wheel_cpp/src/
COPY dynamic_script_engine/ dynamic_script_engine/
COPY node_macros/src/ node_macros/src/
COPY zihuan_core/src/ zihuan_core/src/
COPY zihuan_service/src/ zihuan_service/src/
COPY zihuan_service/build.rs zihuan_service/build.rs
COPY zihuan_service/tests/ zihuan_service/tests/
COPY zihuan_workspace_service/src/ zihuan_workspace_service/src/
COPY zihuan_ims_service/src/ zihuan_ims_service/src/
COPY zihuan_cli/src/ zihuan_cli/src/

# Copy pre-built frontend into webui/dist so the Rust build.rs can find it
COPY --from=frontend-builder /build/webui/dist webui/dist

# Avoid rebuilding an already-produced frontend bundle in the Rust builder image.
ENV ZIHUAN_SKIP_FRONTEND_BUILD=1

RUN cargo build --release -p zihuan_service --bin zihuan_next

# ---- Runtime ----
FROM debian:bookworm-slim

# libstdc++6 is required by the Node binary copied from the frontend stage.
# python3 (3.11 on bookworm) + python3-venv back the default `uv_project`
# runtime of the dynamic script engine; uv is pinned to the system interpreter
# so the first run creates its venv without downloading anything.
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libgcc-s1 \
    libstdc++6 \
    python3 \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

COPY --from=frontend-builder /usr/local/bin/node /usr/local/bin/node
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv
COPY --from=ghcr.io/astral-sh/uv:latest /uvx /usr/local/bin/uvx
ENV UV_PYTHON_PREFERENCE=only-system \
    UV_PYTHON_DOWNLOADS=never

WORKDIR /app

# Same layout as the CI release package: the workspace root is the process CWD
# and the script engine resolves dag_nodes/ and package.json relative to it.
COPY --from=backend-builder /build/target/release/zihuan_next /app/zihuan_next
COPY package.json /app/package.json
COPY build_support/release/pyproject.toml /app/pyproject.toml
COPY dag_nodes/ /app/dag_nodes/
COPY sub_agents/ /app/sub_agents/
COPY scheduled_jobs/ /app/scheduled_jobs/
COPY dynamic_script_engine/engine.mjs \
     dynamic_script_engine/engine_runtime.py \
     dynamic_script_engine/zihuan_sdk.mjs \
     dynamic_script_engine/zihuan_sdk.py \
     dynamic_script_engine/package.json \
     /app/dynamic_script_engine/

ENV ZIHUAN_HOST=0.0.0.0
ENV ZIHUAN_PORT=9951

EXPOSE 9951

CMD ["./zihuan_next"]
