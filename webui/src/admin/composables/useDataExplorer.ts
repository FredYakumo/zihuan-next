import { computed, onMounted, ref } from "vue";

import {
  explorer,
  system,
  type MysqlRecord,
  type ServiceExplorerImageRecord,
  type ServiceExplorerMemoryRecord,
  type ServiceWithRuntime,
} from "../../api/client";

export type ServiceCapability = "messages" | "memories" | "images";

function stringConfig(service: ServiceWithRuntime, key: string): string {
  const value = service.agent_type[key];
  return typeof value === "string" ? value.trim() : "";
}

export function serviceCapabilities(service: ServiceWithRuntime): ServiceCapability[] {
  const type = service.agent_type.type;
  const hasMessages = type === "qq_chat" && Boolean(
    stringConfig(service, "rdb_id") || stringConfig(service, "mysql_connection_id") || stringConfig(service, "task_db_connection_id"),
  );
  const hasMemories = Boolean(
    stringConfig(service, "weaviate_memory_connection_id") || stringConfig(service, "elasticsearch_memory_connection_id"),
  );
  const hasImages = type === "qq_chat" && Boolean(
    stringConfig(service, "weaviate_image_connection_id") || stringConfig(service, "elasticsearch_image_connection_id"),
  );
  return [hasMessages ? "messages" : null, hasMemories ? "memories" : null, hasImages ? "images" : null]
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
    const currentService = service.value;
    const connectionId = currentService ? stringConfig(currentService, "weaviate_memory_connection_id") : "";
    const embeddingModelRefId = currentService ? stringConfig(currentService, "embedding_model_ref_id") : "";
    if (!connectionId || !embeddingModelRefId || !memory.value.mutable) return;
    if (kind === "delete") {
      if (item && window.confirm("确认删除这条记忆吗？")) await explorer.deleteAgentMemory(connectionId, item.object_id);
      await searchMemories();
      return;
    }
    const title = window.prompt("记忆标题", item?.title ?? "");
    if (!title?.trim()) return;
    const value = window.prompt("记忆内容", item?.value ?? "");
    if (!value?.trim()) return;
    const expiresAt = window.prompt("过期时间 RFC3339，可留空表示永久", item?.expires_at ?? "");
    const sender = window.prompt("sender_id_list，多个用逗号分隔，可留空", item?.sender_id_list.join(",") ?? "");
    const group = window.prompt("group_id_list，多个用逗号分隔，可留空", item?.group_id_list.join(",") ?? "");
    const payload = {
      title: title.trim(), value: value.trim(), expires_at: expiresAt?.trim() || null,
      sender_id_list: (sender ?? "").split(",").map((value) => value.trim()).filter(Boolean),
      group_id_list: (group ?? "").split(",").map((value) => value.trim()).filter(Boolean),
    };
    if (kind === "create") await explorer.createAgentMemory(connectionId, embeddingModelRefId, payload);
    if (kind === "edit" && item) await explorer.updateAgentMemory(connectionId, embeddingModelRefId, item.object_id, payload);
    await searchMemories();
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

  return { service, loadingService, unavailable, message, messageTotalPages, searchMessages, changeMessagePage, memory, searchMemories, image, searchImages, mutateMemory };
}
