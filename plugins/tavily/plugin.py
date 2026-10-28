import re

import requests
from tavily import TavilyClient

PLUGIN_MANIFEST = {
    "plugin_type": "web_search_engine",
    "name": "tavily",
    "display_name": "Tavily",
    "description": "使用Tavily 服务商进行网络搜索",
    "version": "0.1.0",
    "config_fields": [
        {"name": "api_token", "label": "API Token", "description": "服务商控制台签发的访问令牌", "optional": False, "secret": True},
    ],
}

USER_AGENT = "Mozilla/5.0 (compatible; zihuan-next/1.0)"


def _timeout(config: dict) -> float:
    try:
        return float(config.get("timeout_secs") or 30)
    except (TypeError, ValueError):
        return 30.0


def _client(config: dict) -> TavilyClient:
    token = (config.get("api_token") or "").strip()
    if not token:
        raise RuntimeError("Tavily 连接缺少 API Token，请在连接配置中填写")
    return TavilyClient(api_key=token)


def _score(value) -> str:
    return f"\nScore: {value:.4f}" if isinstance(value, (int, float)) else ""


def search(args: dict, config: dict) -> dict:
    response = _client(config).search(
        query=args.get("query") or "",
        max_results=int(args.get("search_count") or 3),
        search_depth="advanced",
        include_answer=False,
        include_images=False,
        include_raw_content=False,
        timeout=_timeout(config),
    )
    return {
        "results": [
            f"标题: {item.get('title', '')}\n链接: {item.get('url', '')}"
            f"{_score(item.get('score'))}\n内容: {item.get('content', '')}"
            for item in response.get("results") or []
        ]
    }


def extract_url(args: dict, config: dict) -> dict:
    response = _client(config).extract(
        urls=args.get("url") or "",
        extract_depth="advanced",
        format="markdown",
        include_images=False,
        include_favicon=False,
        timeout=_timeout(config),
    )
    results = response.get("results") or []
    if not results:
        raise RuntimeError(
            f"Tavily extract returned no successful results: {response.get('failed_results')}"
        )
    return {
        "results": [
            f"链接: {item.get('url', '')}\n内容: {item.get('raw_content', '')}" for item in results
        ]
    }


def search_images(args: dict, config: dict) -> dict:
    response = _client(config).search(
        query=args.get("query") or "",
        max_results=int(args.get("max_results") or 3),
        search_depth="basic",
        include_answer=False,
        include_images=True,
        include_image_descriptions=True,
        include_raw_content=False,
        timeout=_timeout(config),
    )
    return {
        "images": [
            {"url": image.get("url", ""), "description": image.get("description")}
            for image in response.get("images") or []
            if isinstance(image, dict)
        ]
    }


def fetch_url_direct(args: dict, config: dict) -> dict:
    url = (args.get("url") or "").strip()
    response = requests.get(url, headers={"User-Agent": USER_AGENT}, timeout=_timeout(config))
    response.raise_for_status()
    return {"results": [f"链接: {url}\n内容: {_plain_text(response.text)}"]}


def _plain_text(body: str) -> str:
    return " ".join(re.sub(r"<[^>]*>", " ", body).split())
