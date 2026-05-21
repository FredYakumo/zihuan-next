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
COPY zihuan_core/Cargo.toml zihuan_core/Cargo.toml
COPY zihuan_agent/Cargo.toml zihuan_agent/Cargo.toml
COPY zihuan_graph_engine/Cargo.toml zihuan_graph_engine/Cargo.toml
COPY zihuan_graph_cli/Cargo.toml zihuan_graph_cli/Cargo.toml
COPY model_inference/Cargo.toml model_inference/Cargo.toml
COPY ims_bot_adapter/Cargo.toml ims_bot_adapter/Cargo.toml
COPY storage_handler/Cargo.toml storage_handler/Cargo.toml
COPY zihuan_service/Cargo.toml zihuan_service/Cargo.toml
COPY zihuan_nlp/Cargo.toml zihuan_nlp/Cargo.toml

# Create stub source files so cargo can resolve the workspace
RUN mkdir -p node_macros/src general_wheel_cpp/src zihuan_core/src zihuan_agent/src \
    zihuan_graph_engine/src zihuan_graph_cli/src model_inference/src \
    ims_bot_adapter/src storage_handler/src zihuan_service/src zihuan_nlp/src src && \
    echo "fn main() {}" > node_macros/src/lib.rs && \
    echo "fn main() {}" > general_wheel_cpp/src/lib.rs && \
    echo "fn main() {}" > zihuan_core/src/lib.rs && \
    echo "fn main() {}" > zihuan_agent/src/lib.rs && \
    echo "fn main() {}" > zihuan_graph_engine/src/lib.rs && \
    echo "fn main() {}" > zihuan_graph_cli/src/main.rs && \
    echo "fn main() {}" > model_inference/src/lib.rs && \
    echo "fn main() {}" > ims_bot_adapter/src/lib.rs && \
    echo "fn main() {}" > storage_handler/src/lib.rs && \
    echo "fn main() {}" > zihuan_service/src/lib.rs && \
    echo "fn main() {}" > zihuan_nlp/src/lib.rs && \
    echo "fn main() {}" > src/main.rs

# Patch crate for third_party
COPY third_party/ third_party/

RUN cargo build --release 2>/dev/null || true

# copy actual source and C++ native code, then do the real build
COPY build.rs ./
COPY general_wheel_cpp/build.rs general_wheel_cpp/build.rs
COPY general_wheel_cpp/native/ general_wheel_cpp/native/
COPY general_wheel_cpp/src/ general_wheel_cpp/src/
COPY node_macros/src/ node_macros/src/
COPY zihuan_core/src/ zihuan_core/src/
COPY zihuan_agent/src/ zihuan_agent/src/
COPY zihuan_graph_engine/src/ zihuan_graph_engine/src/
COPY zihuan_graph_cli/src/ zihuan_graph_cli/src/
COPY model_inference/src/ model_inference/src/
COPY model_inference/build.rs model_inference/build.rs
COPY model_inference/models/ model_inference/models/
COPY ims_bot_adapter/src/ ims_bot_adapter/src/
COPY storage_handler/src/ storage_handler/src/
COPY zihuan_service/src/ zihuan_service/src/
COPY zihuan_service/build.rs zihuan_service/build.rs
COPY zihuan_service/tests/ zihuan_service/tests/
COPY zihuan_nlp/src/ zihuan_nlp/src/
COPY src/ src/

# Copy pre-built frontend into webui/dist so the Rust build.rs can find it
COPY --from=frontend-builder /build/webui/dist webui/dist

# Touch main.rs to force re-link with real sources
RUN find src -name "*.rs" -exec touch {} +

RUN cargo build --release

# ---- Runtime ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libgcc-s1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend-builder /build/target/release/zihuan_next /app/zihuan_next
COPY config.yaml.example /app/config.yaml

ENV ZIHUAN_HOST=0.0.0.0
ENV ZIHUAN_PORT=9951

EXPOSE 9951

CMD ["./zihuan_next"]
