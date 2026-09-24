#!/usr/bin/env bash
# Build the zihuan_next binary and package a distributable release archive
# with the same layout as the CI release package (see .github/workflows/build.yml).
#
# Usage: build.sh [cpu|cuda|metal]
#   Linux : cpu | cuda
#   macOS : cpu | metal
# Omit the argument for an interactive prompt.
#
# Environment overrides:
#   SKIP_FRONTEND=1  reuse the existing webui/dist instead of running pnpm
#   SKIP_BUILD=1     repackage the existing target/release binary without cargo
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPO_ROOT"

case "$(uname -s)" in
    Linux*) os_name="Linux"; variants=(cpu cuda) ;;
    Darwin*) os_name="macOS"; variants=(cpu metal) ;;
    MINGW*|MSYS*|CYGWIN*)
        echo "zihuan-next: on Windows use scripts/build-release.ps1 instead." >&2
        exit 1
        ;;
    *)
        echo "zihuan-next: unsupported OS: $(uname -s)" >&2
        exit 1
        ;;
esac

VARIANT="${1:-}"
if [ -z "$VARIANT" ]; then
    echo "zihuan-next: select build variant"
    i=1
    for v in "${variants[@]}"; do
        echo "  $i) $v"
        i=$((i + 1))
    done
    read -r -p "Enter choice [1]: " choice
    case "$choice" in
        2) VARIANT="${variants[1]}" ;;
        *) VARIANT="${variants[0]}" ;;
    esac
fi

allowed=0
for v in "${variants[@]}"; do
    if [ "$VARIANT" = "$v" ]; then allowed=1; fi
done
if [ "$allowed" -ne 1 ]; then
    echo "zihuan-next: invalid variant '$VARIANT' for $os_name (allowed: ${variants[*]})" >&2
    exit 1
fi
echo "zihuan-next: variant=$VARIANT"

if ! command -v cargo >/dev/null 2>&1; then
    echo "zihuan-next: cargo not found on PATH. Install Rust via rustup." >&2
    exit 1
fi
if [ "${SKIP_FRONTEND:-0}" != "1" ] && ! command -v pnpm >/dev/null 2>&1; then
    echo "zihuan-next: pnpm not found on PATH. Install pnpm (e.g. corepack enable)." >&2
    exit 1
fi
if [ "$VARIANT" = "cuda" ] && [ "${SKIP_BUILD:-0}" != "1" ] && ! command -v nvcc >/dev/null 2>&1; then
    echo "zihuan-next: nvcc not found on PATH. Install the CUDA toolkit." >&2
    exit 1
fi

binary="target/release/zihuan_next"

if [ "${SKIP_FRONTEND:-0}" = "1" ]; then
    if [ ! -d "webui/dist" ]; then
        echo "zihuan-next: webui/dist not found; run without SKIP_FRONTEND first." >&2
        exit 1
    fi
    echo "zihuan-next: skipping webui build (SKIP_FRONTEND=1)"
else
    echo "zihuan-next: building webui"
    (cd webui && pnpm install --frozen-lockfile && pnpm run build)
fi

if [ "${SKIP_BUILD:-0}" = "1" ]; then
    if [ ! -f "$binary" ]; then
        echo "zihuan-next: $binary not found; run without SKIP_BUILD first." >&2
        exit 1
    fi
    echo "zihuan-next: skipping cargo build (SKIP_BUILD=1)"
else
    case "$VARIANT" in
        cpu)
            echo "zihuan-next: building CPU binary"
            cargo build --release -p zihuan_service --bin zihuan_next
            ;;
        cuda)
            echo "zihuan-next: building CUDA binary"
            cargo build --release -p zihuan_service --bin zihuan_next --features candle-cuda
            ;;
        metal)
            echo "zihuan-next: building Metal binary"
            cargo build --release -p zihuan_service --bin zihuan_next --features candle-metal
            ;;
    esac
fi

version="$(git describe --tags --always --dirty 2>/dev/null || echo dev)"

package_root="package/zihuan_next"
rm -rf "$package_root"
mkdir -p "$package_root/dag_nodes" "$package_root/dynamic_script_engine" "$package_root/sub_agents" "$package_root/scheduled_jobs"
cp "$binary" "$package_root/"
cp package.json "$package_root/"
cp -R dag_nodes/. "$package_root/dag_nodes/"
cp -R sub_agents/. "$package_root/sub_agents/"
cp -R scheduled_jobs/. "$package_root/scheduled_jobs/"
cp dynamic_script_engine/{engine.mjs,engine_runtime.py,zihuan_sdk.mjs,zihuan_sdk.py,package.json} "$package_root/dynamic_script_engine/"
cp build_support/release/pyproject.toml "$package_root/pyproject.toml"
find "$package_root" -depth -type d -name "__pycache__" -exec rm -rf {} +

case "$os_name" in
    Linux)
        archive="package/zihuan_next-$version-Linux-x86_64-$VARIANT.tar.xz"
        tar -cJf "$archive" -C package zihuan_next
        ;;
    macOS)
        archive="package/zihuan_next-$version-macOS-Apple-$VARIANT.zip"
        (cd package && zip -qr "$(basename "$archive")" zihuan_next)
        ;;
esac

echo "zihuan-next: package created: $archive"
