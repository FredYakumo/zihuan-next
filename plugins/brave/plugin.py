import re

import requests
from brave import Brave

PLUGIN_MANIFEST = {
    "plugin_type": "web_search_engine",
    "name": "brave",
    "display_name": "Brave",
    "description": "使用Brave 服务商进行网络搜索",
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


class _TimedBrave(Brave):
    """Brave client that honours a timeout and lets HTTP errors reach the caller."""

    def __init__(self, api_key: str, timeout: float) -> None:
        super().__init__(api_key=api_key)
        self._timeout = timeout

    def _get(self, params=None):
        response = requests.get(
            self.base_url + self.endpoint + "/search",
            headers=self._prepare_headers(),
            params=params,
            timeout=self._timeout,
        )
        response.raise_for_status()
        return response


def _client(config: dict) -> Brave:
    token = (config.get("api_token") or "").strip()
    if not token:
        raise RuntimeError("Brave 连接缺少 API Token，请在连接配置中填写")
    return _TimedBrave(api_key=token, timeout=_timeout(config))


def search(args: dict, config: dict) -> dict:
    response = _client(config).search(
        q=args.get("query") or "",
        count=int(args.get("search_count") or 3),
        extra_snippets=True,
        raw=True,
    )
    results = []
    for item in ((response or {}).get("web") or {}).get("results") or []:
        content = item.get("description") or ""
        snippets = item.get("extra_snippets") or []
        if snippets:
            content += "\n额外片段: " + "\n".join(snippets)
        score = item.get("score")
        score_text = f"\nScore: {score:.4f}" if isinstance(score, (int, float)) else ""
        results.append(
            f"标题: {item.get('title', '')}\n链接: {item.get('url', '')}{score_text}\n内容: {content}"
        )
    return {"results": results}


def fetch_url_direct(args: dict, config: dict) -> dict:
    url = (args.get("url") or "").strip()
    response = requests.get(url, headers={"User-Agent": USER_AGENT}, timeout=_timeout(config))
    response.raise_for_status()
    return {"results": [f"链接: {url}\n内容: {_plain_text(response.text)}"]}


def _plain_text(body: str) -> str:
    return " ".join(re.sub(r"<[^>]*>", " ", body).split())
