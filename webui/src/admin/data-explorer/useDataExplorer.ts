import { computed, onMounted, ref } from "vue";

import {
  explorer,
  system,
  type MysqlRecord,
  type ServiceExplorerImageRecord,
  type ServiceExplorerMemoryRecord,
  type ServiceWithRuntime,
} from "../../api/client";

export type ServiceCapability = "messages" | "memories" | "images" | "contexts";

function stringConfig(service: ServiceWithRuntime, key: string): string {
  const value = service.role_service_type[key];
  return typeof value === "string" ? value.trim() : "";
}

function retrievalStoreConnectionId(service: ServiceWithRuntime): string {
  const store = service.role_service_type.retrieval_store as
    | { type?: string; connection_id?: string }
    | undefined;
  return store?.type === "connection" && typeof store.connection_id === "string"
    ? store.connection_id.trim()
    : "";
}

export function serviceCapabilities(service: ServiceWithRuntime): ServiceCapability[] {
  const type = service.role_service_type.type;
  const hasMessages = type === "qq_chat" && Boolean(
    stringConfig(service, "rdb_id") || stringConfig(service, "mysql_connection_id") || stringConfig(service, "task_db_connection_id"),
  );
  const hasRetrievalStore = Boolean(retrievalStoreConnectionId(service));
  // The single retrieval store serves both agent memory and image semantic search.
  const hasMemories = hasRetrievalStore;
  const hasImages = type === "qq_chat" && hasRetrievalStore;
  // The context view reads the running agent's conversation cache; a QQ chat service needs its
  // own relational database configured (same gate as chat records).
  const hasContexts = type === "qq_chat" && Boolean(stringConfig(service, "rdb_id") || stringConfig(service, "mysql_connection_id"));
  return [hasMessages ? "messages" : null, hasMemories ? "memories" : null, hasImages ? "images" : null, hasContexts ? "contexts" : null]
    .filter((item): item is ServiceCapability => item !== null);
}

export function useDataExplorerList() {
  const services = ref<ServiceWithRuntime[]>([]);
  const explorerServices = computed(() => services.value.filter((service) => serviceCapabilities(service).length > 0));

  onMounted(async () => {
    try {
      services.value = await system.services.list();
    } catch (error: unknown) {
      alert((error as Error).message);
    }
  });

  return { explorerServices };
}

export function useDataExplorerDetail(serviceId: string, capability: ServiceCapability) {
  const service = ref<ServiceWithRuntime | null>(null);
  const loadingService = ref(true);
  const unavailable = computed(() => !loadingService.value && (!service.value || !serviceCapabilities(service.value).includes(capability)));

  const message = ref({
    loading: false,
    searched: false,
    records: [] as MysqlRecord[],
    total: 0,
    page: 1,
    pageSize: 20,
    filters: { message_id: "", sender_id: "", sender_name: "", group_id: "", content: "", send_time_start: "", send_time_end: "" },
  });
  const memory = ref({ loading: false, searched: false, query: "", limit: 20, mutable: false, items: [] as ServiceExplorerMemoryRecord[] });
  const memoryDialog = ref({
    visible: false,
    mode: "create" as "create" | "edit",
    saving: false,
    objectId: "",
    form: { title: "", value: "", expires_at: "", sender_id_list: "", group_id_list: "" },
    errors: { title: "", value: "" },
  });
  const image = ref({ loading: false, searched: false, nameQuery: "", descriptionQuery: "", limit: 20, items: [] as ServiceExplorerImageRecord[] });
  const messageTotalPages = computed(() => Math.max(1, Math.ceil(message.value.total / message.value.pageSize)));

  async function searchMessages() {
    if (unavailable.value || capability !== "messages") return;
    message.value.loading = true;
    message.value.searched = true;
    try {
      const response = await explorer.queryServiceMessages({ service_id: serviceId, ...message.value.filters, page: message.value.page, page_size: message.value.pageSize });
      message.value.records = response.records;
      message.value.total = response.total;
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      message.value.loading = false;
    }
  }

  function changeMessagePage(page: number) {
    message.value.page = Math.max(1, Math.min(page, messageTotalPages.value));
    void searchMessages();
  }

  async function searchMemories() {
    if (unavailable.value || capability !== "memories") return;
    memory.value.loading = true;
    memory.value.searched = true;
    try {
      const response = await explorer.queryServiceMemories({ service_id: serviceId, query: memory.value.query.trim() || undefined, limit: memory.value.limit });
      memory.value.items = response.items;
      memory.value.mutable = response.mutable;
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      memory.value.loading = false;
    }
  }

  async function searchImages() {
    if (unavailable.value || capability !== "images") return;
    image.value.loading = true;
    image.value.searched = true;
    try {
      const response = await explorer.queryServiceImages({ service_id: serviceId, name_query: image.value.nameQuery.trim() || undefined, description_query: image.value.descriptionQuery.trim() || undefined, limit: image.value.limit });
      image.value.items = response.items;
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      image.value.loading = false;
    }
  }

  async function mutateMemory(kind: "create" | "edit" | "delete", item?: ServiceExplorerMemoryRecord) {
    if (!memory.value.mutable) return;
    const currentService = service.value;
    const connectionId = currentService ? retrievalStoreConnectionId(currentService) : "";
    if (!connectionId) {
      alert("当前 Service 未配置记忆检索连接");
      return;
    }
    if (kind === "delete") {
      if (!item || !window.confirm("确认删除这条记忆吗？")) return;
      try {
        await explorer.deleteAgentMemory(connectionId, item.object_id);
        await searchMemories();
      } catch (error: unknown) {
        alert((error as Error).message);
      }
      return;
    }
    const embeddingModelRefId = currentService ? stringConfig(currentService, "embedding_model_ref_id") : "";
    if (!embeddingModelRefId) {
      alert("创建或编辑记忆前请先为 Service 选择 Text Embedding 模型");
      return;
    }
    memoryDialog.value = {
      visible: true,
      mode: kind,
      saving: false,
      objectId: item?.object_id ?? "",
      form: {
        title: item?.title ?? "",
        value: item?.value ?? "",
        expires_at: item?.expires_at ?? "",
        sender_id_list: item?.sender_id_list?.join(",") ?? "",
        group_id_list: item?.group_id_list?.join(",") ?? "",
      },
      errors: { title: "", value: "" },
    };
  }

  async function submitMemoryDialog() {
    const dialog = memoryDialog.value;
    const currentService = service.value;
    const connectionId = currentService ? retrievalStoreConnectionId(currentService) : "";
    const embeddingModelRefId = currentService ? stringConfig(currentService, "embedding_model_ref_id") : "";
    if (!dialog.visible || dialog.saving || !connectionId || !embeddingModelRefId) return;
    const title = dialog.form.title.trim();
    const value = dialog.form.value.trim();
    dialog.errors = { title: title ? "" : "请输入记忆标题", value: value ? "" : "请输入记忆内容" };
    if (dialog.errors.title || dialog.errors.value) return;
    dialog.saving = true;
    const payload = {
      title,
      value,
      expires_at: dialog.form.expires_at.trim() || null,
      sender_id_list: dialog.form.sender_id_list.split(",").map((entry) => entry.trim()).filter(Boolean),
      group_id_list: dialog.form.group_id_list.split(",").map((entry) => entry.trim()).filter(Boolean),
    };
    try {
      if (dialog.mode === "create") await explorer.createAgentMemory(connectionId, embeddingModelRefId, payload);
      else await explorer.updateAgentMemory(connectionId, embeddingModelRefId, dialog.objectId, payload);
      dialog.visible = false;
      await searchMemories();
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      dialog.saving = false;
    }
  }

  onMounted(async () => {
    try {
      service.value = (await system.services.list()).find((item) => item.config_id === serviceId) ?? null;
      if (!unavailable.value) {
        if (capability === "messages") await searchMessages();
        if (capability === "memories") await searchMemories();
        if (capability === "images") await searchImages();
      }
    } catch (error: unknown) {
      alert((error as Error).message);
    } finally {
      loadingService.value = false;
    }
  });

  return { service, loadingService, unavailable, message, messageTotalPages, searchMessages, changeMessagePage, memory, memoryDialog, searchMemories, image, searchImages, mutateMemory, submitMemoryDialog };
}
