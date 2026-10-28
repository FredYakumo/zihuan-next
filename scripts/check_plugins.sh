#!/usr/bin/env bash
# Checks zihuan-next dynamic script plugins (syntax + plugin conventions).
# Usage: scripts/check_plugins.sh [plugin-or-directory] [--full]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PYTHON=""
for candidate in python3 python; do
    # The executable test filters out the Windows Store python stub, which exists on PATH
    # but only prints an install hint and exits non-zero.
    if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import sys' >/dev/null 2>&1; then
        PYTHON="$candidate"
        break
    fi
done
if [ -z "$PYTHON" ]; then
    echo "check_plugins: 未找到可用的 python3，请先安装 Python" >&2
    exit 1
fi

exec "$PYTHON" "$SCRIPT_DIR/check_plugins.py" "$@"
