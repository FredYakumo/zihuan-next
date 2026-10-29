"""把 Markdown 渲染成可分享的网页预览，作为 brain tool 提供给 Agent。

渲染页（hubenchang0515.github.io/markdown-editor）从 `?content=` 或 `#content=` 读取内容，
`content` 是「percent 编码后的 Markdown」再做 base64。这里固定走 hash：长文档的 URL 会超过静态
站点允许的请求行长度（414 URI Too Long），hash 不会发给服务器。

短链和截图都依赖外部服务，失败时降级成可用的链接并说明原因，而不是让整次调用失败。
"""

import base64
import hashlib
import json
import os
import urllib.parse

import requests

PLUGIN_MANIFEST = {
    "plugin_type": "brain_tool",
    "name": "markdown_preview",
    "display_name": "Markdown 预览",
    "description": "把 Markdown 渲染成可分享的网页预览链接，并尽量生成预览截图",
    "version": "0.1.0",
    "tool": {
        "name": "render_markdown",
        "description": (
            "把 Markdown（代码块、表格、标题、列表、长文档等）渲染成网页预览。"
            "需要格式化输出时必须使用这个工具。"
            "返回的 preview_url 是给用户的链接，把它放进最终回复。"
            "返回的 images 里如果有 media_id，再用 [Image media_id=...] 把预览图一起发出去。"
        ),
        "parameters": {
            "type": "object",
            "properties": {
                "markdown": {
                    "type": "string",
                    "description": "要渲染的 Markdown 全文",
                },
                "preview_image": {
                    "type": "boolean",
                    "description": "是否生成预览截图，默认 true；只要链接时设为 false 更快",
                },
            },
            "required": ["markdown"],
        },
    },
}

DEFAULT_EDITOR_URL = "https://hubenchang0515.github.io/markdown-editor/#content="
# 短链走红萌（hmoexyz.qzz.io）：它返回 302 真跳转，国内也能直连。
DEFAULT_SHORTLINK_ENDPOINT = "https://hmoexyz.qzz.io/api/shortlink/put"
DEFAULT_SHORTLINK_PAGE = "https://hmoexyz.qzz.io/s/"
# 短链服务会校验请求来源，缺少这两个头会直接 403。
SHORTLINK_HEADERS = {
    "Content-Type": "application/json",
    "Origin": "https://hmoexyz.qzz.io",
    "Referer": "https://hmoexyz.qzz.io/",
}
DEFAULT_SCREENSHOT_ENDPOINT = "https://api.microlink.io/"
SCREENSHOT_WAIT_MS = 4000
SCREENSHOT_WIDTH = 900
# 内容要经过 URL 再经过截图服务的请求，过长会在很下游才失败，这里先拦下来。
MAX_MARKDOWN_CHARS = 20000


def _setting(name: str, default: str) -> str:
    value = os.environ.get(name, "").strip()
    return value or default


def _timeout(config: dict) -> float:
    try:
        return float(config.get("timeout_secs") or 60)
    except (TypeError, ValueError):
        return 60.0


def encode_markdown(markdown: str) -> str:
    """渲染页要求的编码：先 percent 编码，再 base64。"""
    escaped = urllib.parse.quote(markdown, safe="")
    return base64.b64encode(escaped.encode("utf-8")).decode("ascii")


def build_editor_url(markdown: str) -> str:
    editor_url = _setting("MARKDOWN_PREVIEW_EDITOR_URL", DEFAULT_EDITOR_URL)
    return editor_url + urllib.parse.quote(encode_markdown(markdown), safe="")


def shorten(url: str, timeout: float) -> str:
    endpoint = _setting("MARKDOWN_PREVIEW_SHORTLINK_ENDPOINT", DEFAULT_SHORTLINK_ENDPOINT)
    page = _setting("MARKDOWN_PREVIEW_SHORTLINK_PAGE", DEFAULT_SHORTLINK_PAGE)
    response = requests.post(
        endpoint, json={"url": url}, headers=SHORTLINK_HEADERS, timeout=timeout
    )
    response.raise_for_status()
    payload = response.json() or {}
    code = payload.get("code")
    if not isinstance(code, str) or not code.strip():
        raise RuntimeError(f"短链服务没有返回 code：{payload}")
    return page + code.strip()


def screenshot_image_url(target: str, timeout: float) -> str:
    """让截图服务渲染 target，返回它托管好的图片地址。

    截图的是渲染页本身而不是短链：短链靠 JS 跳转，截图服务等不到跳转完成。
    `embed=screenshot.url` 会重定向到图片本身，所以最终地址可以直接下载。
    """
    endpoint = _setting("MARKDOWN_PREVIEW_SCREENSHOT_ENDPOINT", DEFAULT_SCREENSHOT_ENDPOINT)
    params = {
        "url": target,
        "screenshot": "true",
        "meta": "false",
        "embed": "screenshot.url",
        "waitForTimeout": str(SCREENSHOT_WAIT_MS),
        "viewport.width": str(SCREENSHOT_WIDTH),
        "fullPage": "true",
    }
    api_key = os.environ.get("MARKDOWN_PREVIEW_SCREENSHOT_API_KEY", "").strip()
    if api_key:
        params["apiKey"] = api_key
    response = requests.get(endpoint, params=params, timeout=timeout)
    content_type = response.headers.get("content-type", "")
    if response.status_code != 200 or not content_type.startswith("image/"):
        detail = response.text[:200].replace("\n", " ") if response.text else ""
        raise RuntimeError(f"截图服务返回 {response.status_code} {content_type} {detail}".strip())
    if not response.content:
        raise RuntimeError("截图服务返回了空图片")
    return response.url


def preview_media(image_url: str) -> dict:
    """按宿主的工具结果约定描述预览图。

    宿主的回复管线会读取工具结果里的 `images`，模型用 `[Image media_id=...]` 引用后，
    发送时按 `original_source` 下载这张图；这里不落对象存储，所以 `rustfs_path` 留空。
    """
    digest = hashlib.sha1(image_url.encode("utf-8")).hexdigest()[:16]
    return {
        "media_id": f"markdown-preview-{digest}",
        "source": "agent_save",
        "original_source": image_url,
        "rustfs_path": "",
        "name": "markdown-preview.png",
        "description": "Markdown preview screenshot",
        "mime_type": "image/png",
    }


def execute(args: dict, config: dict) -> str:
    markdown = args.get("markdown")
    if not isinstance(markdown, str) or not markdown.strip():
        raise RuntimeError("markdown 不能为空")
    if len(markdown) > MAX_MARKDOWN_CHARS:
        raise RuntimeError(
            f"markdown 内容过长（{len(markdown)} 字符，上限 {MAX_MARKDOWN_CHARS}），请拆分后再渲染"
        )

    timeout = _timeout(config)
    markdown_url = build_editor_url(markdown)
    preview_url = markdown_url
    notes = []

    try:
        preview_url = shorten(markdown_url, timeout)
    except Exception as error:  # 短链只是让链接更好看，失败时仍返回完整链接
        notes.append(f"短链生成失败，改用完整链接：{error}")

    images = []
    if args.get("preview_image") is not False:
        try:
            images.append(preview_media(screenshot_image_url(markdown_url, timeout)))
        except Exception as error:  # 截图依赖外部服务，失败不影响链接
            notes.append(f"预览截图生成失败：{error}")

    return json.dumps(
        {
            "ok": True,
            "preview_url": preview_url,
            "markdown_url": markdown_url,
            "markdown_chars": len(markdown),
            "images": images,
            "notes": notes,
        },
        ensure_ascii=False,
    )
