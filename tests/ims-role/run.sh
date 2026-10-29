#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
cargo build -p zihuan_service --bin zihuan_next
python3 tests/ims-role/run.py "$@"
