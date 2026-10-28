import re

import requests

PLUGIN_MANIFEST = {
    "plugin_type": "web_search_engine",
    "name": "deepseek",
    "display_name": "DeepSeek",
    "description": "使用DeepSeek Web Search进行网络搜索",
    "version": "0.1.0",
    "config_fields": [
        {"name": "api_token", "label": "API Token", "description": "服务商控制台签发的访问令牌", "optional": False, "secret": True},
    ],
}

SEARCH_ENDPOINT = "https://api.deepseek.com/v1/tools/search"
FETCH_ENDPOINT = "https://api.deepseek.com/v1/tools/fetch"
USER_AGENT = "Mozilla/5.0 (compatible; zihuan-next/1.0)"


def _timeout(config: dict) -> float:
    try:
        return float(config.get("timeout_secs") or 30)
    except (TypeError, ValueError):
        return 30.0


def _headers(config: dict) -> dict:
    token = (config.get("api_token") or "").strip()
    if not token:
        raise RuntimeError("DeepSeek 连接缺少 API Token，请在连接配置中填写")
    return {"Authorization": f"Bearer {token}", "User-Agent": USER_AGENT}


def _post_json(config: dict, url: str, body: dict) -> dict:
    response = requests.post(url, json=body, headers=_headers(config), timeout=_timeout(config))
    response.raise_for_status()
    return response.json()


def _non_empty(value) -> str | None:
    if isinstance(value, str) and value.strip():
        return value
    return None


def search(args: dict, config: dict) -> dict:
    try:
        count = int(args.get("search_count") or 3)
    except (TypeError, ValueError):
        count = 3
    response = _post_json(
        config,
        SEARCH_ENDPOINT,
        {"query": args.get("query") or "", "count": max(1, min(50, count)), "need_content": False},
    )
    results = []
    for item in response.get("results") or []:
        content = (
            _non_empty(item.get("summary"))
            or _non_empty(item.get("snippet"))
            or _non_empty(item.get("content"))
            or ""
        )
        date = _non_empty(item.get("date"))
        date_text = f"\n发布时间: {date}" if date else ""
        results.append(
            f"标题: {item.get('title', '')}\n链接: {item.get('url', '')}{date_text}\n内容: {content}"
        )
    return {"results": results}


def extract_url(args: dict, config: dict) -> dict:
    url = (args.get("url") or "").strip()
    response = _post_json(config, FETCH_ENDPOINT, {"url": url})
    title = _non_empty(response.get("title"))
    title_text = f"标题: {title}\n" if title else ""
    return {"results": [f"{title_text}链接: {url}\n内容: {response.get('content', '')}"]}


def fetch_url_direct(args: dict, config: dict) -> dict:
    url = (args.get("url") or "").strip()
    response = requests.get(url, headers={"User-Agent": USER_AGENT}, timeout=_timeout(config))
    response.raise_for_status()
    return {"results": [f"链接: {url}\n内容: {_plain_text(response.text)}"]}


def _plain_text(body: str) -> str:
    return " ".join(re.sub(r"<[^>]*>", " ", body).split())
