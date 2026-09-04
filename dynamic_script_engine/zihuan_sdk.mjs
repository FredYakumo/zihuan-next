/** @typedef {import("./zihuan_sdk.d.ts").NodeDefinition} NodeDefinition */
/** @typedef {import("./zihuan_sdk.d.ts").ZihuanSdk} ZihuanSdkContract */

/**
 * Declares a static or dynamically resolved port on a script node.
 *
 * @param {string} name Stable port name used by graph edges and inputs.
 * @param {import("./zihuan_sdk.d.ts").DataType} data_type Runtime data type.
 * @param {{ required?: boolean, hidden?: boolean, description?: string | null }} [options={}] UI and validation options.
 * @returns {import("./zihuan_sdk.d.ts").PortDefinition} Serializable port definition.
 */
export const port = (name, data_type, options = {}) => ({
  name,
  data_type,
  required: options.required ?? true,
  hidden: options.hidden ?? false,
  description: options.description ?? null,
});

/**
 * Opaque reference to a Rust-owned resource exposed to dynamic scripts.
 *
 * Handles are intentionally immutable. They may be passed back through ZiHuan
 * calls, but must not be dereferenced or reconstructed by script code.
 */
export class ResourceHandle {
  /**
   * @param {string} handle Runtime-scoped opaque handle ID.
   * @param {string} dataType ZiHuan data type associated with the handle.
   */
  constructor(handle, dataType) {
    this.handle = handle;
    this.dataType = dataType;
    Object.freeze(this);
  }

  /**
   * Converts this handle to the JSON wire format understood by the Rust host.
   *
   * @returns {{ $zihuan_handle: string, data_type: string }} Opaque resource handle payload.
   */
  toJSON() {
    return { $zihuan_handle: this.handle, data_type: this.dataType };
  }
}

export class RedisRef extends ResourceHandle {}
export class RdbRef extends ResourceHandle {}
export class S3Ref extends ResourceHandle {}
export class WeaviateRef extends ResourceHandle {}
export class WebSearchEngineRef extends ResourceHandle {}
export class SessionStateRef extends ResourceHandle {}
export class LLMMessageSessionCacheRef extends ResourceHandle {}
export class LLModel extends ResourceHandle {}
export class EmbeddingModel extends ResourceHandle {}
export class BotAdapterRef extends ResourceHandle {}

const resourceTypes = new Map([
  ["RedisRef", RedisRef], ["RdbRef", RdbRef], ["S3Ref", S3Ref], ["WeaviateRef", WeaviateRef],
  ["WebSearchEngineRef", WebSearchEngineRef], ["SessionStateRef", SessionStateRef],
  ["LLMMessageSessionCacheRef", LLMMessageSessionCacheRef], ["LLModel", LLModel],
  ["EmbeddingModel", EmbeddingModel], ["BotAdapterRef", BotAdapterRef],
]);

/**
 * Converts one Rust wire-format resource handle to its typed ZiHuan wrapper.
 *
 * Non-resource values are returned unchanged so this function can be used as
 * the base case of recursive hydration.
 *
 * @template T
 * @param {T} value Value received from the Rust host.
 * @returns {T | ResourceHandle} Original value or a typed resource handle.
 */
export function resourceFromWire(value) {
  if (!value || typeof value !== "object" || !value.$zihuan_handle || !value.data_type) return value;
  const Resource = resourceTypes.get(value.data_type) ?? ResourceHandle;
  return new Resource(value.$zihuan_handle, value.data_type);
}

/**
 * Recursively replaces Rust resource-handle payloads within arrays and objects.
 *
 * @template T
 * @param {T} value Value received from the Rust host.
 * @returns {T | ResourceHandle} Value tree containing typed resource handles.
 */
export function hydrateResources(value) {
  const resource = resourceFromWire(value);
  if (resource !== value) return resource;
  if (Array.isArray(value)) return value.map(hydrateResources);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.entries(value).map(([key, nested]) => [key, hydrateResources(nested)]));
}

/**
 * Validates that a ZiHuan API argument is the expected resource-handle subclass.
 *
 * @param {unknown} value Candidate resource handle.
 * @param {typeof ResourceHandle} Resource Expected handle constructor.
 * @param {string} dataType Expected ZiHuan data type for error reporting.
 * @returns {ResourceHandle} Validated handle.
 * @throws {TypeError} When the supplied value has an incompatible type.
 */
function requireResource(value, Resource, dataType) {
  if (!(value instanceof Resource)) throw new TypeError(`expected ${dataType} resource handle`);
  return value;
}

/** @implements {ZihuanSdkContract} */
export class ZihuanSdk {
  /**
   * Creates the capability facade injected into a dynamic script node.
   *
   * @param {(method: string, params?: Record<string, unknown>) => Promise<unknown>} request
   * transports a named ZiHuan capability call to the Rust host.
   */
  constructor(request) {
    this._request = async (method, params = {}) => hydrateResources(await request(method, params));
    this.ui = Object.freeze({
      publish: (state) => this._request("ui.publish", { state }),
      update: (patch) => this._request("ui.update", { patch }),
      waitEvent: (eventName, timeoutMs) => this._request("ui.wait_event", { event: eventName ?? null, timeout_ms: timeoutMs ?? 30000 }),
    });
    this.variables = Object.freeze({
      get: (name) => this._request("variables.get", { name }),
      set: (name, value) => this._request("variables.set", { name, value }),
    });
    this.task = Object.freeze({
      progress: (message) => this._request("task.progress", { message }),
      append: (taskId, message) => this._request("task.append", { task_id: taskId, message }),
    });
    this.session = Object.freeze({
      get: (sessionRef, senderId) => this._request("session.get", { session_ref: sessionRef, sender_id: senderId }),
      clear: (sessionRef, senderId) => this._request("session.clear", { session_ref: sessionRef, sender_id: senderId }),
      tryClaim: (sessionRef, senderId, stateJson) => this._request("session.try_claim", { session_ref: sessionRef, sender_id: senderId, state_json: stateJson }),
      release: (sessionRef, senderId) => this._request("session.release", { session_ref: sessionRef, sender_id: senderId }),
    });
    this.messageCache = Object.freeze({
      append: (cacheRef, senderId, messages) => this._request("message_cache.append", { cache_ref: cacheRef, sender_id: senderId, messages }),
      get: (cacheRef, senderId, fallback = []) => this._request("message_cache.get", { cache_ref: cacheRef, sender_id: senderId, fallback }),
      set: (cacheRef, senderId, messages) => this._request("message_cache.set", { cache_ref: cacheRef, sender_id: senderId, messages }),
      clear: (cacheRef, senderId) => this._request("message_cache.clear", { cache_ref: cacheRef, sender_id: senderId }),
    });
    this.models = Object.freeze({
      infer: (llmModel, messages) => this._request("model.llm_infer", { llm_model: llmModel, messages }),
      compactContext: (llmModel, messages, compactContextLength, forceCompact = false) => this._request("model.compact_context", { llm_model: llmModel, messages, compact_context_length: compactContextLength, force_compact: forceCompact }),
      fromRef: (llmRefId) => this._request("model.create_llm_from_ref", { llm_ref_id: llmRefId }),
    });
    this.embeddings = Object.freeze({
      infer: (embeddingModel, text) => this._request("embedding.infer", { embedding_model: embeddingModel, text }),
      batchInfer: (embeddingModel, texts) => this._request("embedding.batch_infer", { embedding_model: embeddingModel, texts }),
      createRemote: (options) => this._request("embedding.create_remote", options),
      createLocal: (modelName) => this._request("embedding.create_local", { model_name: modelName }),
    });
    this.search = Object.freeze({
      createProvider: (configId) => this._request("search.create_provider", { config_id: configId }),
      query: (tavilyRef, query, searchCount) => this._request("search.query", { tavily_ref: tavilyRef, query, search_count: searchCount }),
      web: (reference, options = {}) => this._request("search.web", { web_search_engine_ref: reference, ...options }),
    });
    this.storage = Object.freeze({
      redis: (configId) => this._request("storage.create_redis", { config_id: configId }),
      mysql: (configId) => this._request("storage.create_mysql", { config_id: configId }),
      sqlite: (configId) => this._request("storage.create_sqlite", { config_id: configId }),
      s3: (configId) => this._request("storage.create_s3", { config_id: configId }),
      weaviate: (configId) => this._request("storage.create_weaviate", { config_id: configId }),
      userHistory: (rdbRef, senderId, groupId, limit) => this._request("storage.user_history", { rdb_ref: rdbRef, sender_id: senderId, group_id: groupId, limit }),
      groupHistory: (rdbRef, groupId, limit) => this._request("storage.group_history", { rdb_ref: rdbRef, group_id: groupId, limit }),
      searchMessages: (rdbRef, filters) => this._request("storage.search_messages", { rdb_ref: rdbRef, ...filters }),
      persistQQMessageVectors: (weaviateRef, embeddingModel, messages, metadata) => this._request("storage.persist_qq_message_vectors", { weaviate_ref: weaviateRef, embedding_model: embeddingModel, qq_message_list: messages, ...metadata }),
      persistQQMessageRdb: (rdbRef, messages, metadata) => this._request("storage.persist_qq_message_rdb", { rdb_ref: rdbRef, qq_message_list: messages, ...metadata }),
      persistImageVector: (weaviateRef, request) => this._request("storage.persist_image_vector", { weaviate_ref: weaviateRef, ...request }),
      searchImages: (weaviateRef, embeddingModel, query, options) => this._request("storage.search_images", { weaviate_ref: weaviateRef, embedding_model: embeddingModel, query, ...options }),
    });
    this.agent = Object.freeze({
      llm: (kind) => this._request("agent.llm", { llm_kind: kind }),
      embeddingModel: () => this._request("agent.embedding_model"),
      task: () => this._request("agent.task"),
      rdb: () => this._request("agent.rdb"),
      s3: () => this._request("agent.s3"),
      imageWeaviate: () => this._request("agent.image_weaviate"),
      webSearch: () => this._request("agent.web_search"),
    });
    this.bot = Object.freeze({
      adapter: (configId) => this._request("bot.adapter", { config_id: configId }),
      senderFromEvent: (messageEvent) => this._request("bot.sender_from_event", { message_event: messageEvent }),
      senderIdFromEvent: (messageEvent) => this._request("bot.sender_id_from_event", { message_event: messageEvent }),
      groupIdFromEvent: (messageEvent) => this._request("bot.group_id_from_event", { message_event: messageEvent }),
      optionalGroupIdFromEvent: (messageEvent) => this._request("bot.optional_group_id_from_event", { message_event: messageEvent }),
      messagesFromEvent: (messageEvent) => this._request("bot.messages_from_event", { message_event: messageEvent }),
      filterEventType: (messageEvent, filterType) => this._request("bot.filter_event_type", { message_event: messageEvent, filter_type: filterType }),
      send: (adapter, sender, message) => this._request("bot.send", { ims_bot_adapter: adapter, sender, message }),
      sendBatches: (adapter, targetId, messageBatches, options = {}) => this._request("bot.send_batches", { ims_bot_adapter: adapter, target_id: targetId, message_batches: messageBatches, ...options }),
      extractMessages: (adapter, messageEvent, options = {}) => this._request("bot.extract_messages", { ims_bot_adapter: adapter, message_event: messageEvent, ...options }),
    });
    this.resources = Object.freeze({
      redis: (value) => requireResource(value, RedisRef, "RedisRef"),
      rdb: (value) => requireResource(value, RdbRef, "RdbRef"),
      s3: (value) => requireResource(value, S3Ref, "S3Ref"),
      weaviate: (value) => requireResource(value, WeaviateRef, "WeaviateRef"),
      webSearch: (value) => requireResource(value, WebSearchEngineRef, "WebSearchEngineRef"),
      sessionState: (value) => requireResource(value, SessionStateRef, "SessionStateRef"),
      messageCache: (value) => requireResource(value, LLMMessageSessionCacheRef, "LLMMessageSessionCacheRef"),
      llmModel: (value) => requireResource(value, LLModel, "LLModel"),
      embeddingModel: (value) => requireResource(value, EmbeddingModel, "EmbeddingModel"),
      botAdapter: (value) => requireResource(value, BotAdapterRef, "BotAdapterRef"),
    });
    Object.freeze(this);
  }
}

/**
 * Creates the ZiHuan runtime facade for one node execution context.
 *
 * @param {(method: string, params?: Record<string, unknown>) => Promise<unknown>} request
 * Function that transports a named ZiHuan capability call to the Rust host.
 * @returns {ZihuanSdk} Frozen ZiHuan namespace tree for the executing node.
 */
export function createZihuanSdk(request) {
  return new ZihuanSdk(request);
}
