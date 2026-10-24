import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import type { PrimaryTableCol } from "tdesign-vue-next";

import type { MysqlRecord, ServiceChatUserItem } from "../../api/client";
import { type ServiceCapability, useDataExplorerDetail } from "./useDataExplorer";
import { useChatContext } from "./useChatContext";

export function useDataExplorerDetailPage() {
  const route = useRoute();
  const router = useRouter();
  const serviceId = String(route.params.serviceId ?? "");
  const capability = String(route.params.capability ?? "") as ServiceCapability;
  const { service, loadingService, unavailable, message, searchMessages, changeMessagePage, memory, searchMemories, image, searchImages, mutateMemory } = useDataExplorerDetail(serviceId, capability);
  const context = useChatContext(serviceId);
  if (capability === "contexts") context.loadUsers();
  const { backend, loadingUsers, users, selectedSenderId, selectedTitle, loadingContext, clearingContext, turns, archivedTurns, estimatedContextTokens, contextLimitTokens, contextStartedAt, lastMessageAt, hasActiveTurn, inputTokens, outputTokens, averageTokensPerSecond, averageCacheHitRate, loadUsers, clearSelection, clearContext } = context;

  const userColumns: PrimaryTableCol<ServiceChatUserItem>[] = [{ colKey: "sender", title: "用户", width: 220 }, { colKey: "group", title: "最近群组", width: 180 }, { colKey: "context_tokens", title: "上下文 Token", width: 130 }, { colKey: "last", title: "最近时间", width: 200 }];
  const messageColumns: PrimaryTableCol<MysqlRecord>[] = [{ colKey: "message_id", title: "消息 ID", width: 160 }, { colKey: "sender", title: "发送者", width: 160 }, { colKey: "send_time", title: "发送时间", width: 180 }, { colKey: "group", title: "群组", width: 150 }, { colKey: "content", title: "内容" }];

  const pageTitle = computed(() => capability === "messages" ? "聊天记录" : capability === "memories" ? "记忆" : capability === "contexts" ? "上下文" : "图片库");

  function onUserRowClick({ row }: { row: ServiceChatUserItem }) { void context.selectUser(row); }
  function serviceTypeLabel(type: string) { return type === "qq_chat" ? "QQ Chat" : type === "http_stream" ? "HTTP Stream" : "Workspace"; }
  function matchLabel(kind: string) {
    return kind === "keyword" ? "关键词" : kind === "semantic" ? "语义" : kind === "hybrid" ? "混合" : "最近更新";
  }
  function onMessagePaginationChange(info: { current: number; pageSize: number }) { message.value.pageSize = info.pageSize; changeMessagePage(info.current); }
  function returnToList() { void router.push("/data-explorer"); }

  const failedPreviews = ref(new Set<string>());
  function onImageError(objectId: string) { failedPreviews.value.add(objectId); }
  watch(() => image.value.items, () => { failedPreviews.value = new Set<string>(); });

  return {
    serviceId,
    capability,
    service,
    loadingService,
    unavailable,
    message,
    messageColumns,
    searchMessages,
    onMessagePaginationChange,
    memory,
    searchMemories,
    mutateMemory,
    image,
    searchImages,
    failedPreviews,
    onImageError,
    pageTitle,
    userColumns,
    onUserRowClick,
    serviceTypeLabel,
    matchLabel,
    returnToList,
    backend,
    loadingUsers,
    users,
    selectedSenderId,
    selectedTitle,
    loadingContext,
    clearingContext,
    turns,
    archivedTurns,
    estimatedContextTokens,
    contextLimitTokens,
    contextStartedAt,
    lastMessageAt,
    hasActiveTurn,
    inputTokens,
    outputTokens,
    averageTokensPerSecond,
    averageCacheHitRate,
    loadUsers,
    clearSelection,
    clearContext,
  };
}
