"""Typed JSON-RPC SDK available to ZiHuan Python tools.

The host owns resources. Python receives and returns opaque resource handles only;
use the SDK namespaces instead of constructing protocol messages directly.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from typing import Any, Optional


@dataclass(frozen=True)
class ResourceHandle:
    handle: str
    data_type: str

    def to_json(self) -> dict[str, str]:
        return {"$zihuan_handle": self.handle, "data_type": self.data_type}


class RedisRef(ResourceHandle): pass
class RdbRef(ResourceHandle): pass
class S3Ref(ResourceHandle): pass
class WeaviateRef(ResourceHandle): pass
class WebSearchEngineRef(ResourceHandle): pass
class SessionStateRef(ResourceHandle): pass
class LLMMessageSessionCacheRef(ResourceHandle): pass
class LLModel(ResourceHandle): pass
class EmbeddingModel(ResourceHandle): pass
class BotAdapterRef(ResourceHandle): pass


_RESOURCE_TYPES = {
    "RedisRef": RedisRef,
    "RdbRef": RdbRef,
    "S3Ref": S3Ref,
    "WeaviateRef": WeaviateRef,
    "WebSearchEngineRef": WebSearchEngineRef,
    "SessionStateRef": SessionStateRef,
    "LLMMessageSessionCacheRef": LLMMessageSessionCacheRef,
    "LLModel": LLModel,
    "EmbeddingModel": EmbeddingModel,
    "BotAdapterRef": BotAdapterRef,
}


def hydrate_resources(value: Any) -> Any:
    if isinstance(value, dict) and "$zihuan_handle" in value and "data_type" in value:
        resource_type = _RESOURCE_TYPES.get(value["data_type"], ResourceHandle)
        return resource_type(value["$zihuan_handle"], value["data_type"])
    if isinstance(value, list):
        return [hydrate_resources(item) for item in value]
    if isinstance(value, dict):
        return {key: hydrate_resources(item) for key, item in value.items()}
    return value


def _wire_value(value: Any) -> Any:
    if isinstance(value, ResourceHandle):
        return value.to_json()
    if isinstance(value, list):
        return [_wire_value(item) for item in value]
    if isinstance(value, dict):
        return {key: _wire_value(item) for key, item in value.items()}
    return value


class Host:
    def __init__(self) -> None:
        self._next_id = 0

    def call(self, method: str, params: Optional[dict[str, Any]] = None) -> Any:
        self._next_id += 1
        request_id = self._next_id
        json.dump(
            {
                "kind": "host_request",
                "id": request_id,
                "method": method,
                "params": _wire_value(params or {}),
            },
            sys.stdout,
        )
        sys.stdout.write("\n")
        sys.stdout.flush()
        response = json.loads(sys.stdin.readline())
        if response.get("id") != request_id:
            raise RuntimeError("unexpected host RPC response")
        if response.get("error"):
            raise RuntimeError(response["error"])
        return hydrate_resources(response.get("result"))

    def task_progress(self, message: str) -> bool:
        return self.call("task.progress", {"message": message})

    def get_variable(self, name: str) -> Any:
        return self.call("variables.get", {"name": name})

    def set_variable(self, name: str, value: Any) -> bool:
        return self.call("variables.set", {"name": name, "value": value})

    @property
    def variables(self) -> "_Variables":
        return _Variables(self)


class _Variables:
    def __init__(self, host: Host) -> None:
        self._host = host

    def get(self, name: str) -> Any:
        return self._host.get_variable(name)

    def set(self, name: str, value: Any) -> bool:
        return self._host.set_variable(name, value)


class _Task:
    def __init__(self, host: Host) -> None:
        self._host = host

    def progress(self, message: str) -> bool:
        return self._host.call("task.progress", {"message": message})

    def append(self, task_id: str, message: str) -> bool:
        return self._host.call("task.append", {"task_id": task_id, "message": message})


class _Models:
    def __init__(self, host: Host) -> None:
        self._host = host

    def infer(self, model: LLModel, messages: list[dict[str, Any]]) -> dict[str, Any]:
        return self._host.call("model.llm_infer", {"llm_model": model, "messages": messages})

    def from_ref(self, llm_ref_id: str) -> LLModel:
        return self._host.call("model.create_llm_from_ref", {"llm_ref_id": llm_ref_id})


class _Embeddings:
    def __init__(self, host: Host) -> None:
        self._host = host

    def infer(self, model: EmbeddingModel, text: str) -> dict[str, Any]:
        return self._host.call("embedding.infer", {"embedding_model": model, "text": text})

    def batch_infer(self, model: EmbeddingModel, texts: list[str]) -> dict[str, Any]:
        return self._host.call("embedding.batch_infer", {"embedding_model": model, "texts": texts})


class _Storage:
    def __init__(self, host: Host) -> None:
        self._host = host

    def redis(self, config_id: str) -> RedisRef:
        return self._host.call("storage.create_redis", {"config_id": config_id})

    def mysql(self, config_id: str) -> RdbRef:
        return self._host.call("storage.create_mysql", {"config_id": config_id})

    def sqlite(self, config_id: str) -> RdbRef:
        return self._host.call("storage.create_sqlite", {"config_id": config_id})

    def s3(self, config_id: str) -> S3Ref:
        return self._host.call("storage.create_s3", {"config_id": config_id})

    def weaviate(self, config_id: str) -> WeaviateRef:
        return self._host.call("storage.create_weaviate", {"config_id": config_id})


class _Search:
    def __init__(self, host: Host) -> None:
        self._host = host

    def create_provider(self, config_id: str) -> WebSearchEngineRef:
        return self._host.call("search.create_provider", {"config_id": config_id})

    def query(self, reference: WebSearchEngineRef, query: str, search_count: int) -> dict[str, Any]:
        return self._host.call("search.query", {"tavily_ref": reference, "query": query, "search_count": search_count})


class ZihuanSdk:
    """Named host capabilities shared with the JavaScript DAG-node SDK."""

    def __init__(self, host_client: Host) -> None:
        self.variables = _Variables(host_client)
        self.task = _Task(host_client)
        self.models = _Models(host_client)
        self.embeddings = _Embeddings(host_client)
        self.storage = _Storage(host_client)
        self.search = _Search(host_client)


host = Host()
sdk = ZihuanSdk(host)
