export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type DataType = string | { Vec: DataType } | { [key: string]: DataType };
export type PortDefinition = { name: string; data_type: DataType; required: boolean; hidden: boolean; description: string | null };
export type NodeConfigField = { key: string; data_type: DataType; description?: string; required?: boolean; widget?: string; connection_kind?: string | null };

export class ResourceHandle {
  readonly handle: string;
  readonly dataType: string;
  constructor(handle: string, dataType: string);
  toJSON(): { $zihuan_handle: string; data_type: string };
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

export type LLMMessage = JsonValue;
export type QQMessage = JsonValue;
export type NodeInputs = Record<string, JsonValue | ResourceHandle | undefined>;
export type NodeOutputs = Record<string, JsonValue | ResourceHandle | undefined>;

export interface ZihuanSdk {
  readonly variables: { get(name: string): Promise<JsonValue>; set(name: string, value: JsonValue | ResourceHandle): Promise<boolean> };
  readonly task: { progress(message: string): Promise<boolean>; append(taskId: unknown, message: unknown): Promise<boolean> };
  readonly session: {
    get(sessionRef: SessionStateRef, senderId: string): Promise<{ in_session: boolean; state_json: JsonValue }>;
    clear(sessionRef: SessionStateRef, senderId: string): Promise<boolean>;
    tryClaim(sessionRef: SessionStateRef, senderId: string, stateJson?: JsonValue): Promise<{ claimed: boolean; in_session: boolean; state_json: JsonValue }>;
    release(sessionRef: SessionStateRef, senderId: string): Promise<boolean>;
  };
  readonly messageCache: {
    append(cacheRef: LLMMessageSessionCacheRef, senderId: string, messages: LLMMessage[]): Promise<boolean>;
    get(cacheRef: LLMMessageSessionCacheRef, senderId: string, fallback?: LLMMessage[]): Promise<LLMMessage[]>;
    set(cacheRef: LLMMessageSessionCacheRef, senderId: string, messages: LLMMessage[]): Promise<boolean>;
    clear(cacheRef: LLMMessageSessionCacheRef, senderId: string): Promise<boolean>;
  };
  readonly models: {
    infer(model: LLModel, messages: LLMMessage[]): Promise<{ response: LLMMessage[] }>;
    compactContext(model: LLModel, messages: LLMMessage[], compactContextLength: number, forceCompact?: boolean): Promise<{ messages: LLMMessage[]; did_compact: boolean; estimated_tokens_before: number; estimated_tokens_after: number }>;
    fromRef(llmRefId: string): Promise<LLModel>;
  };
  readonly embeddings: {
    infer(model: EmbeddingModel, text: string): Promise<{ embedding: number[]; dimension: number }>;
    batchInfer(model: EmbeddingModel, texts: string[]): Promise<{ embeddings: number[][]; count: number; dimension: number }>;
    createRemote(options: { model_name: string; api_endpoint: string; api_key?: string; timeout_secs?: number; retry_count?: number }): Promise<EmbeddingModel>;
    createLocal(modelName: string): Promise<EmbeddingModel>;
  };
  readonly search: {
    createProvider(configId: string): Promise<WebSearchEngineRef>;
    query(reference: WebSearchEngineRef, query: string, searchCount: number): Promise<{ results: string[] }>;
    web(reference: WebSearchEngineRef, options?: { query?: string; url?: string; search_count?: number }): Promise<{ results: string[] }>;
  };
  readonly storage: {
    redis(configId: string): Promise<RedisRef>;
    mysql(configId: string): Promise<RdbRef>;
    sqlite(configId: string): Promise<RdbRef>;
    s3(configId: string): Promise<S3Ref>;
    weaviate(configId: string): Promise<WeaviateRef>;
    userHistory(rdbRef: RdbRef, senderId: string, groupId: string | undefined, limit: number): Promise<{ messages: string[] }>;
    groupHistory(rdbRef: RdbRef, groupId: string, limit: number): Promise<{ messages: string[] }>;
    searchMessages(rdbRef: RdbRef, filters: Record<string, JsonValue | undefined>): Promise<{ messages: string[] }>;
    persistQQMessageVectors(weaviateRef: WeaviateRef, embeddingModel: EmbeddingModel, messages: QQMessage[], metadata: { message_id: string; sender_id: string; sender_name: string; group_id?: string; group_name?: string }): Promise<boolean>;
    persistQQMessageRdb(rdbRef: RdbRef, messages: QQMessage[], metadata: { message_id: string; sender_id: string; sender_name: string; group_id?: string; group_name?: string }): Promise<boolean>;
    persistImageVector(weaviateRef: WeaviateRef, request: { object_storage_path: string; description: string; embedding_model?: EmbeddingModel; vector?: number[]; source?: string; media_id?: string; original_source?: string; name?: string; mime_type?: string }): Promise<boolean>;
    searchImages(weaviateRef: WeaviateRef, embeddingModel: EmbeddingModel, query: string, options: { limit: number; max_distance?: number; target_vector?: string }): Promise<{ images: JsonValue[]; has_results: boolean }>;
  };
  readonly agent: {
    llm(kind?: JsonValue): Promise<LLModel>;
    embeddingModel(): Promise<EmbeddingModel>;
    task(): Promise<{ task_id: string; has_task: boolean }>;
    rdb(): Promise<RdbRef>;
    s3(): Promise<S3Ref>;
    imageWeaviate(): Promise<WeaviateRef>;
    webSearch(): Promise<WebSearchEngineRef>;
  };
  readonly bot: {
    adapter(configId: string): Promise<BotAdapterRef>;
    senderFromEvent(messageEvent: JsonValue): Promise<JsonValue>;
    senderIdFromEvent(messageEvent: JsonValue): Promise<string>;
    groupIdFromEvent(messageEvent: JsonValue): Promise<string>;
    optionalGroupIdFromEvent(messageEvent: JsonValue): Promise<string>;
    messagesFromEvent(messageEvent: JsonValue): Promise<QQMessage[]>;
    filterEventType(messageEvent: JsonValue, filterType?: string): Promise<{ true_event?: JsonValue; false_event?: JsonValue }>;
    send(adapter: BotAdapterRef, sender: JsonValue, message: QQMessage[]): Promise<{ success: boolean; message_id: number }>;
    sendBatches(adapter: BotAdapterRef, targetId: string, messageBatches: QQMessage[][], options?: { target_type?: "friend" | "group"; delay_millis?: number }): Promise<{ success: boolean; summary: string; message_ids: number[] }>;
    extractMessages(adapter: BotAdapterRef, messageEvent: JsonValue, options?: { message_id?: number; s3_ref?: S3Ref }): Promise<{ messages: LLMMessage[]; content: string; ref_content: string; is_at_me: boolean; at_target_list: string[] }>;
  };
  readonly resources: {
    redis(value: unknown): RedisRef;
    rdb(value: unknown): RdbRef;
    s3(value: unknown): S3Ref;
    weaviate(value: unknown): WeaviateRef;
    webSearch(value: unknown): WebSearchEngineRef;
    sessionState(value: unknown): SessionStateRef;
    messageCache(value: unknown): LLMMessageSessionCacheRef;
    llmModel(value: unknown): LLModel;
    embeddingModel(value: unknown): EmbeddingModel;
    botAdapter(value: unknown): BotAdapterRef;
  };
}

export interface NodeExecutionContext {
  readonly nodeId: string;
  readonly nodeName: string;
  readonly inputs: NodeInputs;
  readonly inline_values: Record<string, JsonValue | undefined>;
  readonly zihuan: ZihuanSdk;
}
export interface NodeDefinition {
  type_id: string;
  display_name: string;
  category: string;
  description?: string;
  input_ports?: PortDefinition[];
  output_ports?: PortDefinition[];
  dynamic_input_ports?: boolean;
  dynamic_output_ports?: boolean;
  config_fields?: NodeConfigField[];
  resolve_ports?(context: Pick<NodeExecutionContext, "inline_values">): { input_ports?: PortDefinition[]; output_ports?: PortDefinition[] };
  execute(context: NodeExecutionContext): NodeOutputs | Promise<NodeOutputs>;
}

export function port(name: string, dataType: DataType, options?: { required?: boolean; hidden?: boolean; description?: string | null }): PortDefinition;
export function resourceFromWire(value: JsonValue): JsonValue | ResourceHandle;
export function hydrateResources(value: JsonValue): JsonValue | ResourceHandle;
export class ZihuanSdk { constructor(request: (method: string, params?: Record<string, JsonValue | ResourceHandle | undefined>) => Promise<JsonValue>); }
export function createZihuanSdk(request: (method: string, params?: Record<string, JsonValue | ResourceHandle | undefined>) => Promise<JsonValue>): ZihuanSdk;
