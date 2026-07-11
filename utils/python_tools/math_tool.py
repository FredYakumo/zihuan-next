def run_tool(request: dict) -> dict:
    arguments = request.get("arguments") or {}
    left = arguments.get("left", 0)
    right = arguments.get("right", 0)
    total = left + right
    return {
        "ok": True,
        "result": {
            "sum": total,
            "summary": f"{left} + {right} = {total}",
        },
        "error": None,
    }
