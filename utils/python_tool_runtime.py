import argparse
import importlib.util
import json
import sys
import traceback
from pathlib import Path


def load_module(script_path: Path):
    spec = importlib.util.spec_from_file_location("zihuan_python_tool", script_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load python tool module: {script_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--script", required=True)
    parser.add_argument("--entry", required=True)
    args = parser.parse_args()

    script_path = Path(args.script).resolve()
    try:
      request = json.load(sys.stdin)
      module = load_module(script_path)
      entry = getattr(module, args.entry, None)
      if entry is None or not callable(entry):
          raise RuntimeError(f"entry function not found: {args.entry}")
      result = entry(request)
      if not isinstance(result, dict):
          raise RuntimeError("python tool result must be a dict")
      response = {
          "ok": bool(result.get("ok", False)),
          "result": result.get("result"),
          "error": result.get("error"),
      }
    except Exception as exc:
      response = {
          "ok": False,
          "result": None,
          "error": f"{exc}\n{traceback.format_exc()}",
      }

    json.dump(response, sys.stdout, ensure_ascii=False)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
