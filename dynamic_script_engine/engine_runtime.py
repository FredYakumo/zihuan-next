import argparse
import asyncio
import importlib.util
import inspect
import json
import os
import sys
import traceback
from pathlib import Path
from typing import Any

from zihuan_sdk import Host, NodeExecutionContext, ZihuanSdk, hydrate_resources, registered_nodes


ENGINE_DIR = Path(__file__).resolve().parent
NODE_DIR = Path(os.environ.get("ZIHUAN_DAG_NODES", ENGINE_DIR.parent / "dag_nodes"))
SCHEDULED_JOBS_DIR = Path(os.environ.get("ZIHUAN_SCHEDULED_JOBS", ENGINE_DIR.parent / "scheduled_jobs"))
PLUGIN_METHODS = ("search", "extract_url", "fetch_url_direct", "search_images")


def _display_path(path: Path) -> str:
    try:
        return str(path.relative_to(NODE_DIR))
    except ValueError:
        return str(path)


def load_module(path: Path) -> tuple[Any | None, str | None]:
    from zihuan_sdk import _CURRENT_SCRIPT_PATH
    try:
        try:
            script_path = path.relative_to(Path.cwd())
        except ValueError:
            script_path = path
        sys.path.insert(0, str(path.parent))
        spec = importlib.util.spec_from_file_location(f"zihuan_node_{path.stem}", path)
        if spec is None or spec.loader is None: raise RuntimeError("cannot create module specification")
        module = importlib.util.module_from_spec(spec)
        
        import zihuan_sdk
        zihuan_sdk._CURRENT_SCRIPT_PATH = str(script_path)
        spec.loader.exec_module(module)
        zihuan_sdk._CURRENT_SCRIPT_PATH = None
        return module, None
    except Exception as error:
        return None, f"failed to load {_display_path(path)}: {error}"
    finally:
        if sys.path and sys.path[0] == str(path.parent): sys.path.pop(0)


def load_nodes() -> tuple[dict[str, Any], list[dict[str, str]]]:
    diagnostics: list[dict[str, str]] = []
    if NODE_DIR.is_dir():
        for path in sorted(NODE_DIR.rglob("*.py")):
            _, failure = load_module(path)
            if failure: diagnostics.append({"language": "python", "message": failure})
    nodes = {definition.type_id: definition for definition in registered_nodes()}
    return nodes, diagnostics


def load_jobs(paths: list[str]) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    jobs: list[dict[str, Any]] = []
    diagnostics: list[dict[str, str]] = []
    for relative in paths:
        path = Path(SCHEDULED_JOBS_DIR, relative)
        module, failure = load_module(path)
        if failure: diagnostics.append({"language": "python", "message": failure}); continue
        manifest = getattr(module, "JOB_MANIFEST", None)
        if not isinstance(manifest, dict):
            diagnostics.append({"language": "python", "message": f"{relative} must define a JOB_MANIFEST dict"})
            continue
        jobs.append({"task_name": manifest.get("task_name"), "script": relative, "entry": manifest.get("entry") or "run_job", "description": manifest.get("description") or ""})
    return jobs, diagnostics


def normalize_config_fields(raw: Any) -> list[dict[str, Any]]:
    """Purpose: sanitize a plugin's declared config fields, defaulting every field to required."""
    if not isinstance(raw, list): return []
    fields: list[dict[str, Any]] = []
    for item in raw:
        if isinstance(item, str): item = {"name": item}
        if not isinstance(item, dict) or not item.get("name"): continue
        name = str(item.get("name"))
        if name in ("api_token", "timeout_secs") and name != "api_token": continue
        fields.append({
            "name": name,
            "label": str(item.get("label") or name),
            "description": str(item.get("description") or ""),
            "optional": item.get("optional") is True,
            "secret": bool(item.get("secret")),
        })
    return fields


def load_plugins(paths: list[str]) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    """Purpose: report the PLUGIN_MANIFEST each plugin entry declares, with its implemented methods."""
    plugins: list[dict[str, Any]] = []
    diagnostics: list[dict[str, str]] = []
    for raw in paths:
        path = Path(raw).resolve()
        module, failure = load_module(path)
        if failure: diagnostics.append({"language": "python", "message": failure}); continue
        manifest = getattr(module, "PLUGIN_MANIFEST", None)
        if not isinstance(manifest, dict):
            diagnostics.append({"language": "python", "message": f"{path.name} must define a PLUGIN_MANIFEST dict"})
            continue
        if not manifest.get("plugin_type"):
            diagnostics.append({"language": "python", "message": f"{path.name} PLUGIN_MANIFEST is missing plugin_type"})
            continue
        plugins.append({
            "plugin_type": manifest.get("plugin_type"),
            "name": manifest.get("name") or path.parent.name,
            "display_name": manifest.get("display_name") or manifest.get("name") or path.parent.name,
            "description": manifest.get("description") or "",
            "version": str(manifest.get("version") or ""),
            "methods": [method for method in PLUGIN_METHODS if callable(getattr(module, method, None))],
            "config_fields": normalize_config_fields(manifest.get("config_fields")),
        })
    return plugins, diagnostics


def invoke_plugin(request: dict[str, Any]) -> dict[str, Any]:
    """Purpose: run one plugin method call in the plugin's own environment and return its payload."""
    module, failure = load_module(Path(request["script_path"]).resolve())
    if failure: return {"error": failure}
    func = getattr(module, request.get("method"), None)
    if not callable(func): return {"error": f"插件未实现方法: {request.get('method')}"}
    try:
        result = func(request.get("args") or {}, request.get("config") or {})
        if inspect.isawaitable(result): result = asyncio.run(result)
        return {"response": result}
    except Exception as error:
        return {"error": f"{error}\n{traceback.format_exc()}"}


def definition_json(definition: Any) -> dict[str, Any]:
    return {"type_id": definition.type_id, "display_name": definition.display_name, "category": definition.category, "description": definition.description, "script_path": definition.script_path, "input_ports": [port.to_json() for port in definition.input_ports], "output_ports": [port.to_json() for port in definition.output_ports], "dynamic_input_ports": definition.dynamic_input_ports, "dynamic_output_ports": definition.dynamic_output_ports, "config_fields": definition.config_fields}


def resolve_ports(definition: Any, values: dict[str, Any]) -> dict[str, Any]:
    resolved = definition.resolve_ports(values) if definition.resolve_ports else {}
    return {"input_ports": [port.to_json() for port in resolved.get("input_ports", definition.input_ports)], "output_ports": [port.to_json() for port in resolved.get("output_ports", definition.output_ports)]}


def report_tool_manifest(script_path: str) -> dict[str, Any]:
    """Purpose: report the parameters and outputs a script declares through TOOL_MANIFEST."""
    module, failure = load_module(Path(script_path).resolve())
    if failure: return {"error": failure}
    manifest = getattr(module, "TOOL_MANIFEST", None) or {}
    if not isinstance(manifest, dict): return {"error": "TOOL_MANIFEST must be a dict"}
    return {"parameters": manifest.get("parameters") or [], "outputs": manifest.get("outputs") or []}


def invoke(execute: Any, context: NodeExecutionContext) -> dict[str, Any]:
    result = execute(context)
    if inspect.isawaitable(result): result = asyncio.run(result)
    if not isinstance(result, dict): raise RuntimeError("node execute must return a dict")
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", action="store_true")
    parser.add_argument("--ports", action="store_true")
    parser.add_argument("--serve", action="store_true")
    parser.add_argument("--jobs-catalog", action="store_true")
    parser.add_argument("--plugins-catalog", action="store_true")
    parser.add_argument("--plugin-execute", action="store_true")
    parser.add_argument("--tool-manifest", action="store_true")
    args = parser.parse_args()
    modes = (args.catalog, args.ports, args.serve, args.jobs_catalog, args.plugins_catalog, args.plugin_execute, args.tool_manifest)
    if sum(modes) != 1:
        parser.error("expected exactly one of --catalog, --ports, --serve, --jobs-catalog, --plugins-catalog, --plugin-execute, or --tool-manifest")
    if args.tool_manifest:
        request = json.loads(sys.stdin.read() or "{}")
        print(json.dumps(report_tool_manifest(request["script_path"]), ensure_ascii=False)); return 0
    if args.jobs_catalog:
        request = json.loads(sys.stdin.read() or "{}")
        jobs, diagnostics = load_jobs(request.get("paths", []))
        print(json.dumps({"jobs": jobs, "diagnostics": diagnostics}, ensure_ascii=False)); return 0
    if args.plugins_catalog:
        request = json.loads(sys.stdin.read() or "{}")
        plugins, diagnostics = load_plugins(request.get("paths", []))
        print(json.dumps({"plugins": plugins, "diagnostics": diagnostics}, ensure_ascii=False)); return 0
    if args.plugin_execute:
        print(json.dumps(invoke_plugin(json.loads(sys.stdin.read() or "{}")), ensure_ascii=False)); return 0
    nodes, diagnostics = load_nodes()
    if args.catalog: print(json.dumps({"nodes": [definition_json(node) for node in nodes.values()], "diagnostics": diagnostics}, ensure_ascii=False)); return 0
    if args.ports:
        request = json.loads(sys.stdin.read()); node = nodes.get(request.get("type_id"))
        if node is None: raise RuntimeError(f"unknown DAG node: {request.get('type_id')}")
        print(json.dumps(resolve_ports(node, request.get("inline_values", {})), ensure_ascii=False)); return 0
    for line in sys.stdin:
        try:
            request = json.loads(line)
            if request.get("kind") == "host_response": continue
            if request.get("kind") in ("tool_execute", "script_tool_execute"):
                tool = request["request"]; module, failure = load_module(Path(tool["script_path"]).resolve())
                if failure: raise RuntimeError(failure)
                entry = getattr(module, tool["entry"])
                # A scheduler job script builds its own JobSdk; an agent script tool is handed
                # the ZiHuan SDK facade so it can reach host capabilities directly.
                result = entry(tool) if request["kind"] == "tool_execute" else entry(tool, ZihuanSdk(Host()))
                print(json.dumps({"response": result}, ensure_ascii=False), flush=True); continue
            node = nodes.get(request.get("type_id"))
            if node is None: raise RuntimeError(f"unknown DAG node: {request.get('type_id')}")
            context = NodeExecutionContext(request["node_id"], request["node_name"], hydrate_resources(request.get("inputs", {})), request.get("inline_values", {}), ZihuanSdk(Host()))
            print(json.dumps({"kind": "execute_response", "outputs": invoke(node.execute, context)}, ensure_ascii=False, default=lambda value: value.to_json()), flush=True)
        except Exception as error:
            print(json.dumps({"kind": "execute_response", "error": f"{error}\n{traceback.format_exc()}"}, ensure_ascii=False), flush=True)
    return 0


if __name__ == "__main__": raise SystemExit(main())
