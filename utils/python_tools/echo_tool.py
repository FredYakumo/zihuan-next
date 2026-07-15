def run_tool(request: dict) -> dict:
    arguments = request.get("arguments") or {}
    text = arguments.get("text")
    if text is None:
        return {
            "ok": False,
            "result": None,
            "error": "missing required argument: text",
        }
    return {
        "ok": True,
        "result": {
            "result": f"echo: {text}",
        },
        "error": None,
    }
