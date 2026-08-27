"""Typed SDK injected into ZiHuan Python dynamic nodes and tools."""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from typing import Any, Callable, Protocol, TypeAlias

JsonValue: TypeAlias = str | int | float | bool | None | list["JsonValue"] | dict[str, "JsonValue"]


@dataclass(frozen=True)
class ResourceHandle:
    """Purpose: represent a Rust-owned resource that scripts may pass but never dereference."""
    handle: str
    data_type: str

    def to_json(self) -> dict[str, str]:
        """Purpose: serialize this opaque handle for a later SDK or node-output call."""
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


_RESOURCE_TYPES = {cls.__name__: cls for cls in (RedisRef, RdbRef, S3Ref, WeaviateRef, WebSearchEngineRef, SessionStateRef, LLMMessageSessionCacheRef, LLModel, EmbeddingModel, BotAdapterRef)}


def hydrate_resources(value: Any) -> Any:
    """Purpose: turn resource-handle payloads received from Rust into typed handles."""
    if isinstance(value, dict) and "$zihuan_handle" in value and "data_type" in value:
        cls = _RESOURCE_TYPES.get(str(value["data_type"]), ResourceHandle)
        return cls(str(value["$zihuan_handle"]), str(value["data_type"]))
    if isinstance(value, list): return [hydrate_resources(item) for item in value]
    if isinstance(value, dict): return {key: hydrate_resources(item) for key, item in value.items()}
    return value


def wire_value(value: Any) -> Any:
    if isinstance(value, ResourceHandle): return value.to_json()
    if isinstance(value, list): return [wire_value(item) for item in value]
    if isinstance(value, dict): return {key: wire_value(item) for key, item in value.items()}
    return value


class Host:
    def __init__(self) -> None: self._next_id = 0
    def call(self, method: str, params: dict[str, Any] | None = None) -> Any:
        """Purpose: transport one SDK capability request to the ZiHuan Rust host."""
        self._next_id += 1
        request_id = self._next_id
        json.dump({"kind": "host_request", "id": request_id, "method": method, "params": wire_value(params or {})}, sys.stdout, ensure_ascii=False)
        sys.stdout.write("\n"); sys.stdout.flush()
        response = json.loads(sys.stdin.readline())
        if response.get("id") != request_id: raise RuntimeError("unexpected host RPC response")
        if response.get("error"): raise RuntimeError(str(response["error"]))
        return hydrate_resources(response.get("result"))


class _Namespace:
    def __init__(self, host: Host, prefix: str) -> None: self._host, self._prefix = host, prefix
    def _call(self, name: str, **params: Any) -> Any: return self._host.call(f"{self._prefix}.{name}", params)


class Variables(_Namespace):
    """Purpose: read and update the current graph execution's runtime variables."""
    def get(self, name: str) -> JsonValue:
        """Purpose: read one runtime variable by name, or return null when it is absent."""
        return self._call("get", name=name)
    def set(self, name: str, value: JsonValue | ResourceHandle) -> bool:
        """Purpose: store a JSON value or resource handle for downstream nodes in this graph run."""
        return self._call("set", name=name, value=value)


class Task(_Namespace):
    """Purpose: report progress for the current task or another known task."""
    def progress(self, message: str) -> bool:
        """Purpose: append progress to the task that owns the current graph execution."""
        return self._call("progress", message=message)
    def append(self, task_id: str, message: str) -> bool:
        """Purpose: append progress to an explicitly identified task."""
        return self._call("append", task_id=task_id, message=message)


class Session(_Namespace):
    """Purpose: inspect and atomically claim or release conversation session state."""
    def get(self, session_ref: SessionStateRef, sender_id: str) -> dict[str, Any]: return self._call("get", session_ref=session_ref, sender_id=sender_id)
    def clear(self, session_ref: SessionStateRef, sender_id: str) -> bool: return self._call("clear", session_ref=session_ref, sender_id=sender_id)
    def try_claim(self, session_ref: SessionStateRef, sender_id: str, state_json: JsonValue | None = None) -> dict[str, Any]: return self._call("try_claim", session_ref=session_ref, sender_id=sender_id, state_json=state_json)
    def release(self, session_ref: SessionStateRef, sender_id: str) -> bool: return self._call("release", session_ref=session_ref, sender_id=sender_id)


class MessageCache(_Namespace):
    """Purpose: maintain the per-sender LLM-message cache behind a cache resource handle."""
    def append(self, cache_ref: LLMMessageSessionCacheRef, sender_id: str, messages: list[JsonValue]) -> bool: return self._call("append", cache_ref=cache_ref, sender_id=sender_id, messages=messages)
    def get(self, cache_ref: LLMMessageSessionCacheRef, sender_id: str, fallback: list[JsonValue] | None = None) -> list[JsonValue]: return self._call("get", cache_ref=cache_ref, sender_id=sender_id, fallback=fallback or [])
    def set(self, cache_ref: LLMMessageSessionCacheRef, sender_id: str, messages: list[JsonValue]) -> bool: return self._call("set", cache_ref=cache_ref, sender_id=sender_id, messages=messages)
    def clear(self, cache_ref: LLMMessageSessionCacheRef, sender_id: str) -> bool: return self._call("clear", cache_ref=cache_ref, sender_id=sender_id)


class Models(_Namespace):
    """Purpose: run chat LLM inference and context compaction through an LLModel handle."""
    def infer(self, model: LLModel, messages: list[JsonValue]) -> dict[str, Any]:
        """Purpose: send messages to an LLM and return its normalized response messages."""
        return self._call("llm_infer", llm_model=model, messages=messages)
    def compact_context(self, model: LLModel, messages: list[JsonValue], compact_context_length: int, force_compact: bool = False) -> dict[str, Any]: return self._call("compact_context", llm_model=model, messages=messages, compact_context_length=compact_context_length, force_compact=force_compact)
    def from_ref(self, llm_ref_id: str) -> LLModel: return self._call("create_llm_from_ref", llm_ref_id=llm_ref_id)


class Embeddings(_Namespace):
    """Purpose: create and invoke text embedding models."""
    def infer(self, model: EmbeddingModel, text: str) -> dict[str, Any]: return self._call("infer", embedding_model=model, text=text)
    def batch_infer(self, model: EmbeddingModel, texts: list[str]) -> dict[str, Any]: return self._call("batch_infer", embedding_model=model, texts=texts)
    def create_remote(self, **options: Any) -> EmbeddingModel: return self._call("create_remote", **options)
    def create_local(self, model_name: str) -> EmbeddingModel: return self._call("create_local", model_name=model_name)


class Search(_Namespace):
    """Purpose: create web-search providers and query them through a handle."""
    def create_provider(self, config_id: str) -> WebSearchEngineRef: return self._call("create_provider", config_id=config_id)
    def query(self, reference: WebSearchEngineRef, query: str, search_count: int) -> dict[str, Any]: return self._call("query", tavily_ref=reference, query=query, search_count=search_count)
    def web(self, reference: WebSearchEngineRef, **options: Any) -> dict[str, Any]: return self._call("web", web_search_engine_ref=reference, **options)


class Storage(_Namespace):
    """Purpose: create storage handles and use them for message and vector persistence."""
    def redis(self, config_id: str) -> RedisRef: return self._call("create_redis", config_id=config_id)
    def mysql(self, config_id: str) -> RdbRef: return self._call("create_mysql", config_id=config_id)
    def sqlite(self, config_id: str) -> RdbRef: return self._call("create_sqlite", config_id=config_id)
    def s3(self, config_id: str) -> S3Ref: return self._call("create_s3", config_id=config_id)
    def weaviate(self, config_id: str) -> WeaviateRef: return self._call("create_weaviate", config_id=config_id)
    def user_history(self, rdb_ref: RdbRef, sender_id: str, group_id: str | None, limit: int) -> dict[str, Any]: return self._call("user_history", rdb_ref=rdb_ref, sender_id=sender_id, group_id=group_id, limit=limit)
    def group_history(self, rdb_ref: RdbRef, group_id: str, limit: int) -> dict[str, Any]: return self._call("group_history", rdb_ref=rdb_ref, group_id=group_id, limit=limit)
    def search_messages(self, rdb_ref: RdbRef, **filters: Any) -> dict[str, Any]: return self._call("search_messages", rdb_ref=rdb_ref, **filters)
    def persist_qq_message_vectors(self, weaviate_ref: WeaviateRef, embedding_model: EmbeddingModel, qq_message_list: list[JsonValue], **metadata: Any) -> bool: return self._call("persist_qq_message_vectors", weaviate_ref=weaviate_ref, embedding_model=embedding_model, qq_message_list=qq_message_list, **metadata)
    def persist_qq_message_rdb(self, rdb_ref: RdbRef, qq_message_list: list[JsonValue], **metadata: Any) -> bool: return self._call("persist_qq_message_rdb", rdb_ref=rdb_ref, qq_message_list=qq_message_list, **metadata)
    def persist_image_vector(self, weaviate_ref: WeaviateRef, **request: Any) -> bool: return self._call("persist_image_vector", weaviate_ref=weaviate_ref, **request)
    def search_images(self, weaviate_ref: WeaviateRef, embedding_model: EmbeddingModel, query: str, **options: Any) -> dict[str, Any]: return self._call("search_images", weaviate_ref=weaviate_ref, embedding_model=embedding_model, query=query, **options)


class Agent(_Namespace):
    """Purpose: retrieve resources resolved from the active agent configuration."""
    def llm(self, kind: JsonValue | None = None) -> LLModel: return self._call("llm", llm_kind=kind)
    def embedding_model(self) -> EmbeddingModel: return self._call("embedding_model")
    def task(self) -> dict[str, Any]: return self._call("task")
    def rdb(self) -> RdbRef: return self._call("rdb")
    def s3(self) -> S3Ref: return self._call("s3")
    def image_weaviate(self) -> WeaviateRef: return self._call("image_weaviate")
    def web_search(self) -> WebSearchEngineRef: return self._call("web_search")


class Bot(_Namespace):
    """Purpose: inspect bot events and send or extract platform messages through an adapter handle."""
    def adapter(self, config_id: str) -> BotAdapterRef: return self._call("adapter", config_id=config_id)
    def sender_from_event(self, message_event: JsonValue) -> JsonValue: return self._call("sender_from_event", message_event=message_event)
    def sender_id_from_event(self, message_event: JsonValue) -> str: return self._call("sender_id_from_event", message_event=message_event)
    def group_id_from_event(self, message_event: JsonValue) -> str: return self._call("group_id_from_event", message_event=message_event)
    def optional_group_id_from_event(self, message_event: JsonValue) -> str: return self._call("optional_group_id_from_event", message_event=message_event)
    def messages_from_event(self, message_event: JsonValue) -> list[JsonValue]: return self._call("messages_from_event", message_event=message_event)
    def filter_event_type(self, message_event: JsonValue, filter_type: str | None = None) -> dict[str, Any]: return self._call("filter_event_type", message_event=message_event, filter_type=filter_type)
    def send(self, adapter: BotAdapterRef, sender: JsonValue, message: list[JsonValue]) -> dict[str, Any]: return self._call("send", ims_bot_adapter=adapter, sender=sender, message=message)
    def send_batches(self, adapter: BotAdapterRef, target_id: str, message_batches: list[list[JsonValue]], **options: Any) -> dict[str, Any]: return self._call("send_batches", ims_bot_adapter=adapter, target_id=target_id, message_batches=message_batches, **options)
    def extract_messages(self, adapter: BotAdapterRef, message_event: JsonValue, **options: Any) -> dict[str, Any]: return self._call("extract_messages", ims_bot_adapter=adapter, message_event=message_event, **options)


def _snake(value: str) -> str:
    result = ""
    for index, char in enumerate(value): result += ("_" if index and char.isupper() else "") + char.lower()
    return result


class Resources:
    """Purpose: validate that an input is the expected typed resource handle before SDK use."""
    def __getattr__(self, name: str) -> Callable[[Any], ResourceHandle]:
        names = {"web_search": WebSearchEngineRef, "session_state": SessionStateRef, "message_cache": LLMMessageSessionCacheRef, "llm_model": LLModel, "embedding_model": EmbeddingModel, "bot_adapter": BotAdapterRef}
        cls = names.get(name) or _RESOURCE_TYPES["".join(part.capitalize() for part in _snake(name).split("_"))]
        return lambda value: value if isinstance(value, cls) else (_ for _ in ()).throw(TypeError(f"expected {cls.__name__}"))


class ZihuanSdk:
    """Purpose: expose all Rust-hosted capabilities available to a dynamic Python script."""
    def __init__(self, host: Host) -> None:
        self.variables, self.task = Variables(host, "variables"), Task(host, "task")
        self.session, self.message_cache = Session(host, "session"), MessageCache(host, "message_cache")
        self.models, self.embeddings = Models(host, "model"), Embeddings(host, "embedding")
        self.search, self.storage = Search(host, "search"), Storage(host, "storage")
        self.agent, self.bot, self.resources = Agent(host, "agent"), Bot(host, "bot"), Resources()


@dataclass(frozen=True)
class NodeExecutionContext:
    """Purpose: provide one node invocation's identity, inputs, inline configuration, and SDK."""
    node_id: str
    node_name: str
    inputs: dict[str, Any]
    inline_values: dict[str, Any]
    zihuan: ZihuanSdk


@dataclass(frozen=True)
class Port:
    """Purpose: declare a node input or output port for the graph editor and runtime."""
    name: str
    data_type: Any
    required: bool = True
    hidden: bool = False
    description: str | None = None

    def to_json(self) -> dict[str, Any]: return {"name": self.name, "data_type": self.data_type, "required": self.required, "hidden": self.hidden, "description": self.description}


@dataclass
class NodeDefinition:
    type_id: str
    display_name: str
    category: str
    execute: Callable[[NodeExecutionContext], dict[str, Any]]
    description: str = ""
    input_ports: list[Port] = field(default_factory=list)
    output_ports: list[Port] = field(default_factory=list)
    dynamic_input_ports: bool = False
    dynamic_output_ports: bool = False
    config_fields: list[dict[str, Any]] = field(default_factory=list)
    resolve_ports: Callable[[dict[str, Any]], dict[str, list[Port]]] | None = None


_REGISTERED_NODES: list[NodeDefinition] = []


def node(*, type_id: str, display_name: str, category: str, description: str = "", input_ports: list[Port] | None = None, output_ports: list[Port] | None = None, dynamic_input_ports: bool = False, dynamic_output_ports: bool = False, config_fields: list[dict[str, Any]] | None = None, resolve_ports: Callable[[dict[str, Any]], dict[str, list[Port]]] | None = None) -> Callable[[Callable[[NodeExecutionContext], dict[str, Any]]], Callable[[NodeExecutionContext], dict[str, Any]]]:
    """Purpose: register an execute function as a discoverable ZiHuan Python DAG node."""
    def register(execute: Callable[[NodeExecutionContext], dict[str, Any]]) -> Callable[[NodeExecutionContext], dict[str, Any]]:
        _REGISTERED_NODES.append(NodeDefinition(type_id, display_name, category, execute, description, input_ports or [], output_ports or [], dynamic_input_ports, dynamic_output_ports, config_fields or [], resolve_ports)); return execute
    return register


def registered_nodes() -> list[NodeDefinition]: return list(_REGISTERED_NODES)
