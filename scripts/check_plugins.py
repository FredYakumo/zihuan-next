"""Checks zihuan-next dynamic script plugins for syntax and convention compliance.

The rules mirror the host loader in `dynamic_script_engine/src/lib.rs`
(`installed_plugin_directories`, `prepare_plugin_runtime`, `load_plugin_catalog`) and the
web search engine adapter in `zihuan_core/src/rag/web_search/plugin.rs`:

- A plugin is a directory under `plugins/` that holds `plugin.py`, `plugin.mjs`,
  `plugin.ts` or `plugin.js` (checked in that order). Directory names starting with `_` or
  `.` are helpers, not plugins.
- The entry script must define `PLUGIN_MANIFEST` (`plugin_manifest` for JS/TS) with a
  non-empty `plugin_type`. `config_fields` entries declare connection-level fields;
  `timeout_secs` is reserved and silently dropped by the host.
- A `web_search_engine` plugin must implement the module-level functions `search` and
  `fetch_url_direct`; `extract_url` and `search_images` are optional.
- Python dependencies come from `pyproject.toml` (uv project) or `requirements.txt`
  (dedicated uv venv); a plugin without either runs on the host's own interpreter and may
  only use the standard library. JS/TS plugins have no dependency manifest mechanism and
  import through Node's TypeScript type stripping (Node 22.6+), like the engine does.

Usage:
    python scripts/check_plugins.py [TARGET] [--full]

TARGET is a plugin name under `plugins/`, a plugin directory, a plugin entry file, or a
directory containing several plugins. Without TARGET all of `plugins/` is checked.

Without `--full` the check is static: syntax parsing plus manifest and method validation,
with no dependency installation and no Python code execution. With `--full` every plugin
additionally gets its dependencies installed exactly the way the host does, and is then
loaded through the production runner (`engine_runtime.py --plugins-catalog`), the same code
path the service uses at startup.
"""

from __future__ import annotations

import argparse
import ast
import json
import os
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PLUGINS_DIR = REPO_ROOT / "plugins"
ENGINE_DIR = REPO_ROOT / "dynamic_script_engine"

PLUGIN_ENTRY_FILES = ("plugin.py", "plugin.mjs", "plugin.ts", "plugin.js")
KNOWN_PLUGIN_TYPES = {"web_search_engine"}
PLUGIN_METHODS = ("search", "extract_url", "fetch_url_direct", "search_images")
REQUIRED_METHODS = ("search", "fetch_url_direct")
RESERVED_CONFIG_FIELDS = {"timeout_secs"}

# Runs inside `node --input-type=module`; imports the entry module and reports its
# `plugin_manifest` export plus the plugin methods it actually implements.
JS_PROBE = """
const methods = ["search", "extract_url", "fetch_url_direct", "search_images"];
try {
    const module = await import(process.env.ZIHUAN_PLUGIN_PROBE_URL);
    process.stdout.write(JSON.stringify({
        manifest: module.plugin_manifest === undefined ? null : module.plugin_manifest,
        methods: methods.filter((method) => typeof module[method] === "function"),
    }));
} catch (error) {
    process.stdout.write(JSON.stringify({ error: String(error) }));
}
"""


@dataclass
class Finding:
    level: str
    message: str


@dataclass
class Plugin:
    directory: str
    entry: str
    path: Path

    @property
    def language(self) -> str:
        return "python" if self.entry.endswith(".py") else "javascript"
    @property
    def label(self) -> str:
        return f"{self.directory} ({self.entry})"


@dataclass
class Report:
    plugin: Plugin
    findings: list[Finding] = field(default_factory=list)
    loaded: bool = False

    @property
    def ok(self) -> bool:
        return not any(finding.level == "error" for finding in self.findings)

    def error(self, message: str) -> None:
        self.findings.append(Finding("error", message))

    def warning(self, message: str) -> None:
        self.findings.append(Finding("warning", message))


def discover_plugins(root: Path) -> tuple[list[Plugin], list[str]]:
    """Mirrors installed_plugin_directories: real directories with an entry file, sorted."""
    plugins: list[Plugin] = []
    ignored: list[str] = []
    for path in sorted(root.iterdir()):
        if not path.is_dir() or path.name.startswith(("_", ".")):
            continue
        entry = next((name for name in PLUGIN_ENTRY_FILES if (path / name).is_file()), None)
        if entry is None:
            ignored.append(path.name)
            continue
        plugins.append(Plugin(directory=path.name, entry=entry, path=path))
    return plugins, ignored


def has_dependency_manifest(plugin: Plugin) -> bool:
    return (plugin.path / "pyproject.toml").is_file() or (plugin.path / "requirements.txt").is_file()


def import_roots(node: ast.Import | ast.ImportFrom) -> list[str]:
    if isinstance(node, ast.Import):
        return [alias.name.split(".")[0] for alias in node.names]
    if node.level or not node.module:
        return []
    return [node.module.split(".")[0]]


def callable_with(node: ast.FunctionDef | ast.AsyncFunctionDef, count: int) -> bool:
    arguments = node.args
    positional = [*arguments.posonlyargs, *arguments.args]
    minimum = len(positional) - len(arguments.defaults)
    maximum = len(positional) + (1 if arguments.vararg else 0)
    keyword_required = [arg.arg for arg, default in zip(arguments.kwonlyargs, arguments.kw_defaults) if default is None]
    return minimum <= count <= maximum and not keyword_required


def validate_manifest(report: Report, manifest: dict, implemented: set[str]) -> str | None:
    """Checks one PLUGIN_MANIFEST against the host rules; returns the declared plugin_type."""
    plugin_type = manifest.get("plugin_type")
    if not isinstance(plugin_type, str) or not plugin_type:
        report.error("PLUGIN_MANIFEST 缺少 plugin_type，Host会拒绝收录该Plugin")
        plugin_type = None
    elif plugin_type not in KNOWN_PLUGIN_TYPES:
        report.warning(f"未知 plugin_type '{plugin_type}'：Host会收录它，但没有任何System使用这个类型")

    for key in ("name", "display_name", "description"):
        value = manifest.get(key)
        if value is not None and not isinstance(value, str):
            report.error(f"PLUGIN_MANIFEST 的 {key} 必须是字符串")
    version = manifest.get("version")
    if version is not None and not isinstance(version, (str, int, float)):
        report.error("PLUGIN_MANIFEST 的 version 必须是字符串或数字")

    config_fields = manifest.get("config_fields")
    if config_fields is not None:
        if not isinstance(config_fields, list):
            report.error("PLUGIN_MANIFEST 的 config_fields 必须是数组")
        else:
            declared: set[str] = set()
            for item in config_fields:
                if isinstance(item, str):
                    name = item
                elif isinstance(item, dict) and isinstance(item.get("name"), str) and item["name"]:
                    name = item["name"]
                else:
                    report.warning(f"config_fields 项缺少 name，Host会忽略它: {item!r}")
                    continue
                if name in declared:
                    report.warning(f"config_fields 重复声明 '{name}'")
                declared.add(name)
                if name in RESERVED_CONFIG_FIELDS:
                    report.warning(
                        f"config_fields 声明了保留字段 '{name}'：它映射到连接自身的列，Host会丢弃该声明"
                    )

    if plugin_type in KNOWN_PLUGIN_TYPES:
        for method in REQUIRED_METHODS:
            if method not in implemented:
                report.error(f"web_search_engine Plugin必须实现顶层函数 {method}(args, config)")
    return plugin_type


def check_python_plugin(plugin: Plugin, report: Report) -> None:
    entry = plugin.path / plugin.entry
    try:
        source = entry.read_text(encoding="utf-8")
    except OSError as error:
        report.error(f"无法读取入口文件: {error}")
        return
    try:
        tree = ast.parse(source, filename=str(entry))
    except SyntaxError as error:
        report.error(f"语法错误: 第 {error.lineno} 行第 {error.offset} 列: {error.msg}")
        return

    manifest_node = None
    functions: dict[str, ast.FunctionDef | ast.AsyncFunctionDef] = {}
    external_imports: set[str] = set()
    local_modules = {path.stem for path in plugin.path.glob("*.py")} | {
        path.name for path in plugin.path.iterdir() if path.is_dir()
    }
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            functions[node.name] = node
        elif isinstance(node, ast.Assign):
            if any(isinstance(target, ast.Name) and target.id == "PLUGIN_MANIFEST" for target in node.targets):
                manifest_node = node.value
        elif isinstance(node, ast.AnnAssign):
            if isinstance(node.target, ast.Name) and node.target.id == "PLUGIN_MANIFEST" and node.value is not None:
                manifest_node = node.value
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            for root in import_roots(node):
                if root not in sys.stdlib_module_names and root not in local_modules:
                    external_imports.add(root)

    if external_imports and not has_dependency_manifest(plugin):
        report.warning(
            "顶层导入了非标准库模块 " + ", ".join(sorted(external_imports))
            + "，但目录没有 pyproject.toml / requirements.txt，Host会在共享解释器里运行它"
        )

    if manifest_node is None:
        report.error("缺少 PLUGIN_MANIFEST 定义，Host会拒绝收录该Plugin")
        return
    try:
        manifest = ast.literal_eval(manifest_node)
    except (ValueError, TypeError, SyntaxError):
        report.warning("PLUGIN_MANIFEST 不是静态字面量，无法离线校验规范")
        return
    if not isinstance(manifest, dict):
        report.error("PLUGIN_MANIFEST 必须是字典")
        return
    plugin_type = validate_manifest(report, manifest, set(functions))
    if plugin_type in KNOWN_PLUGIN_TYPES:
        for name in PLUGIN_METHODS:
            if name in functions and not callable_with(functions[name], 2):
                report.error(f"{name} 必须能以 (args, config) 两个位置参数调用")


def node_type_strip_args(node: str) -> list[str]:
    """Mirrors node_type_strip_args in dynamic_script_engine/src/lib.rs: Node 22.18+/23.6+
    strip TypeScript types by default, 22.6 to those releases need the explicit flag, and
    older runtimes cannot import .ts modules at all."""
    try:
        version = subprocess.run(
            [node, "--version"], capture_output=True, text=True, encoding="utf-8", errors="replace"
        ).stdout.strip()
        match = re.match(r"v(\d+)\.(\d+)", version)
        major, minor = int(match.group(1)), int(match.group(2))
    except (OSError, ValueError, AttributeError):
        return []
    if major > 23 or (major == 23 and minor >= 6) or (major == 22 and minor >= 18):
        return []
    if major > 22 or (major == 22 and minor >= 6):
        return ["--experimental-strip-types"]
    return []


def check_javascript_plugin(plugin: Plugin, report: Report) -> None:
    node = shutil.which("node")
    if node is None:
        report.error("未找到 node，无法检测 JS/TS Plugin")
        return
    entry = plugin.path / plugin.entry
    # `node --check` is only trustworthy for .mjs: .js has no unambiguous module type on
    # disk, and for .ts it passes files with real syntax errors. The probe import below
    # compiles the module exactly the way the engine loads it, so for .ts/.js it is the
    # syntax check.
    if plugin.entry.endswith(".mjs"):
        syntax = subprocess.run(
            [node, "--check", str(entry)], capture_output=True, text=True, encoding="utf-8", errors="replace"
        )
        if syntax.returncode != 0:
            report.error(f"语法错误: {(syntax.stderr or syntax.stdout).strip()}")
            return
    probe = subprocess.run(
        [node, *node_type_strip_args(node), "--input-type=module"],
        input=JS_PROBE,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env={**os.environ, "ZIHUAN_PLUGIN_PROBE_URL": entry.as_uri()},
    )
    try:
        payload = json.loads(probe.stdout)
    except ValueError:
        report.error(f"无法加载入口模块: {(probe.stderr or probe.stdout).strip()}")
        return
    if not isinstance(payload, dict):
        report.error(f"入口模块探测输出无效: {probe.stdout.strip()[:200]}")
        return
    if payload.get("error"):
        report.error(f"入口模块加载失败: {payload['error']}")
        return
    manifest = payload.get("manifest")
    if not isinstance(manifest, dict):
        report.error("缺少 plugin_manifest 导出，Host会拒绝收录该Plugin")
        return
    validate_manifest(report, manifest, set(payload.get("methods") or []))


def run_step(argv: list[str], report: Report, what: str) -> bool:
    process = subprocess.run(argv, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if process.returncode == 0:
        return True
    report.error(f"{what}失败: {(process.stderr or process.stdout).strip()}")
    return False


def venv_python_path(plugin_dir: Path) -> Path:
    return plugin_dir / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def full_check(plugin: Plugin, report: Report) -> None:
    """Installs the plugin's dependencies the way prepare_plugin_runtime does, then loads
    the plugin through the production catalog runner."""
    if plugin.language == "python":
        uv = shutil.which("uv")
        if uv is None:
            report.error("未找到 uv，无法按Host方式安装Plugin依赖")
            return
        if (plugin.path / "pyproject.toml").is_file():
            if not run_step([uv, "sync", "--project", str(plugin.path), "--quiet"], report, "依赖安装 (uv sync)"):
                return
            runner = [uv, "run", "--project", str(plugin.path), "--no-sync", "python"]
        elif (plugin.path / "requirements.txt").is_file():
            python = venv_python_path(plugin.path)
            if not python.is_file() and not run_step([uv, "venv", str(plugin.path / ".venv")], report, "环境创建 (uv venv)"):
                return
            if not run_step(
                [uv, "pip", "install", "--python", str(python), "--requirement", str(plugin.path / "requirements.txt")],
                report,
                "依赖安装 (uv pip install)",
            ):
                return
            runner = [str(python)]
        else:
            # No dependency manifest: the host runs the plugin on its own interpreter, which
            # here is the interpreter running the checker. The runner itself (engine_runtime)
            # is standard-library only, and the static check warns about undeclared imports.
            runner = [sys.executable]
    else:
        runner = [shutil.which("node") or "node"]
        runner[1:1] = node_type_strip_args(runner[0])

    runner_file = "engine_runtime.py" if plugin.language == "python" else "engine.mjs"
    process = subprocess.run(
        [*runner, str(ENGINE_DIR / runner_file), "--plugins-catalog"],
        input=json.dumps({"paths": [str(plugin.path / plugin.entry)]}),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        cwd=ENGINE_DIR,
    )
    if process.returncode != 0:
        report.error(f"Host runner 加载失败: {(process.stderr or process.stdout).strip()}")
        return
    try:
        payload = json.loads(process.stdout)
    except ValueError:
        report.error(f"Host runner 输出无法解析: {(process.stdout or process.stderr).strip()[:400]}")
        return
    for diagnostic in payload.get("diagnostics") or []:
        message = diagnostic.get("message") if isinstance(diagnostic, dict) else diagnostic
        report.error(f"Host加载诊断: {message}")
    plugins = payload.get("plugins") or []
    if not plugins:
        if not payload.get("diagnostics"):
            report.error("Host runner 未收录该Plugin（缺少可用的 PLUGIN_MANIFEST）")
        return
    loaded = plugins[0]
    if loaded.get("plugin_type") in KNOWN_PLUGIN_TYPES:
        methods = set(loaded.get("methods") or [])
        missing = [method for method in REQUIRED_METHODS if method not in methods]
        if missing:
            report.error("Host runner 报告的已实现方法缺少 " + ", ".join(missing))
    report.loaded = True


def resolve_target(raw: str) -> Path:
    candidate = Path(raw)
    if candidate.is_absolute():
        return candidate
    local = REPO_ROOT / candidate
    return local if local.exists() else PLUGINS_DIR / candidate


def collect_plugins(target: Path) -> tuple[list[Plugin], list[str]]:
    if target.is_file():
        if target.name in PLUGIN_ENTRY_FILES:
            return [Plugin(directory=target.parent.name, entry=target.name, path=target.parent)], []
        raise SystemExit(f"check_plugins: {target} 不是Plugin入口文件 ({'/'.join(PLUGIN_ENTRY_FILES)})")
    if not target.is_dir():
        raise SystemExit(f"check_plugins: 目标不存在: {target}")
    entry = next((name for name in PLUGIN_ENTRY_FILES if (target / name).is_file()), None)
    if entry is not None:
        return [Plugin(directory=target.name, entry=entry, path=target)], []
    return discover_plugins(target)


def main() -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")

    parser = argparse.ArgumentParser(description="检测 zihuan-next 动态脚本Plugin的语法与Plugin规范")
    parser.add_argument("target", nargs="?", help="Plugin名、Plugin目录、入口文件或Plugin集合目录，默认检测整个 plugins/")
    parser.add_argument(
        "--full", action="store_true", help="安装依赖并用Host runner (engine_runtime.py --plugins-catalog) 实际加载每个Plugin"
    )
    args = parser.parse_args()

    target = resolve_target(args.target) if args.target else PLUGINS_DIR
    plugins, ignored = collect_plugins(target)
    if not plugins:
        print(f"check_plugins: {target} 下没有发现任何Plugin（需要 {' / '.join(PLUGIN_ENTRY_FILES)}）", file=sys.stderr)
        return 1

    print(f"检测Plugin目录: {target}")
    reports: list[Report] = []
    for plugin in plugins:
        report = Report(plugin=plugin)
        if plugin.language == "python":
            check_python_plugin(plugin, report)
        else:
            check_javascript_plugin(plugin, report)
        if args.full:
            full_check(plugin, report)
        reports.append(report)

        status = "通过" if report.ok else "失败"
        suffix = "，并通过Host runner 加载" if args.full and report.ok else ""
        print(f"[{status}] {report.plugin.label}{suffix}")
        for finding in report.findings:
            print(f"    {finding.level.upper()}: {finding.message}")

    if ignored:
        print(
            f"[警告] 以下目录缺少 {' / '.join(PLUGIN_ENTRY_FILES)} 入口，Host不会把它们当作Plugin加载: "
            + ", ".join(ignored)
        )
    failures = [report for report in reports if not report.ok]
    error_count = sum(1 for report in reports for finding in report.findings if finding.level == "error")
    warning_count = sum(1 for report in reports for finding in report.findings if finding.level == "warning")
    print(f"结果: {len(reports) - len(failures)} 通过, {len(failures)} 失败 (error {error_count}, warning {warning_count})")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
