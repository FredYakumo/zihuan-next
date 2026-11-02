import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import type { PrimaryTableCol } from "tdesign-vue-next";

import type { MysqlRecord, MysqlRecordMedia, ServiceChatUserItem } from "../../api/client";
import { type ChatContextFlowExposed } from "./ChatContextFlow";
import { type ServiceCapability, useDataExplorerDetail } from "./useDataExplorer";
import { useChatContext } from "./useChatContext";

export function useDataExplorerDetailPage() {
  const route = useRoute();
  const router = useRouter();
  const serviceId = String(route.params.serviceId ?? "");
  const capability = String(route.params.capability ?? "") as ServiceCapability;
  const { service, loadingService, unavailable, message, searchMessages, changeMessagePage, memory, memoryDialog, searchMemories, image, searchImages, mutateMemory, submitMemoryDialog } = useDataExplorerDetail(serviceId, capability);
  const context = useChatContext(serviceId);
  if (capability === "contexts") context.loadUsers();
  const { backend, loadingUsers, users, selectedSenderId, selectedTitle, loadingContext, clearingContext, deletingTurnId, turns, archivedTurns, estimatedContextTokens, contextLimitTokens, contextStartedAt, lastMessageAt, hasActiveTurn, inputTokens, outputTokens, averageTokensPerSecond, averageCacheHitRate, loadUsers, clearSelection, clearContext, deleteTurn } = context;

  /** Visibility of the conversation finder dialog over the selected user's turns. */
  const turnFinderVisible = ref(false);
  /** The rendered context flow, so a finder pick can scroll it to that turn. */
  const contextFlowRef = ref<ChatContextFlowExposed | null>(null);
  function focusTurn(turnId: string) { contextFlowRef.value?.scrollToTurn(turnId); }

  const userColumns: PrimaryTableCol<ServiceChatUserItem>[] = [{ colKey: "sender", title: "用户", width: 220 }, { colKey: "group", title: "最近群组", width: 180 }, { colKey: "context_tokens", title: "上下文 Token", width: 130 }, { colKey: "last", title: "最近时间", width: 200 }];
  const messageColumns: PrimaryTableCol<MysqlRecord>[] = [{ colKey: "message_id", title: "消息 ID", width: 160 }, { colKey: "sender", title: "发送者", width: 160 }, { colKey: "send_time", title: "发送时间", width: 180 }, { colKey: "group", title: "群组", width: 150 }, { colKey: "content", title: "内容" }, { colKey: "detail", title: "详情", width: 80 }];

  const pageTitle = computed(() => capability === "messages" ? "聊天记录" : capability === "memories" ? "记忆" : capability === "contexts" ? "上下文" : "图片库");

  function onUserRowClick({ row }: { row: ServiceChatUserItem }) { void context.selectUser(row); }
  function serviceTypeLabel(type: string) { return type === "qq_chat" ? "QQ Chat" : type === "http_stream" ? "HTTP Stream" : "Workspace"; }
  function matchLabel(kind: string) {
    return kind === "keyword" ? "关键词" : kind === "semantic" ? "语义" : kind === "hybrid" ? "混合" : "最近更新";
  }
  function formatScore(score: number | null): string {
    return score == null ? "" : String(Number(score.toFixed(4)));
  }
  function onMessagePaginationChange(info: { current: number; pageSize: number }) { message.value.pageSize = info.pageSize; changeMessagePage(info.current); }
  function returnToList() { void router.push("/data-explorer"); }

  const failedPreviews = ref(new Set<string>());
  function onImageError(objectId: string) { failedPreviews.value.add(objectId); }
  watch(() => image.value.items, () => { failedPreviews.value = new Set<string>(); });

  const mediaIdPattern = /\[Image:?\s+media_id=([^\s\]]+)\]/g;

  type MessageContentSegment = { type: "text"; text: string } | { type: "image"; media: MysqlRecordMedia };

  const PREVIEW_TEXT_LIMIT = 150;

  const failedMessageMedia = ref(new Set<string>());
  function onMessageMediaError(mediaId: string) { failedMessageMedia.value.add(mediaId); }
  /** Record keys (`message_id:row`) whose full content is expanded in the messages table. */
  const expandedMessages = ref(new Set<string>());
  watch(() => message.value.records, () => {
    failedMessageMedia.value = new Set<string>();
    expandedMessages.value = new Set<string>();
  });

  function toggleMessageDetail(key: string) {
    const next = expandedMessages.value;
    if (next.has(key)) next.delete(key);
    else next.add(key);
  }
  function isMessageDetailOpen(key: string): boolean {
    return expandedMessages.value.has(key);
  }
  /** The preview keeps only the first characters of text, so longer records offer the detail view. */
  function hasMessageDetail(row: MysqlRecord): boolean {
    return (row.content_full ?? row.content).length > PREVIEW_TEXT_LIMIT;
  }

  /** Interleave a record's full content with the images its `[Image: media_id=...]` placeholders reference. */
  function fullContentSegments(row: MysqlRecord): MessageContentSegment[] {
    const text = row.content_full ?? row.content;
    const available = (row.media ?? []).filter((entry) => entry.url && !failedMessageMedia.value.has(entry.media_id));
    if (!available.length) return text ? [{ type: "text", text }] : [];
    const byId = new Map(available.map((entry) => [entry.media_id, entry]));
    const segments: MessageContentSegment[] = [];
    let cursor = 0;
    for (const match of text.matchAll(mediaIdPattern)) {
      const start = match.index ?? 0;
      if (start > cursor) segments.push({ type: "text", text: text.slice(cursor, start) });
      const entry = byId.get(match[1]);
      if (entry) segments.push({ type: "image", media: entry });
      else segments.push({ type: "text", text: match[0] });
      cursor = start + match[0].length;
    }
    if (cursor < text.length) segments.push({ type: "text", text: text.slice(cursor) });
    return segments;
  }

  /** Cut a segment list down to `limit` characters of text, keeping the images it passes. */
  function limitSegments(segments: MessageContentSegment[], limit: number): MessageContentSegment[] {
    const result: MessageContentSegment[] = [];
    let remaining = limit;
    for (const segment of segments) {
      if (segment.type === "image") {
        result.push(segment);
        continue;
      }
      if (!segment.text) continue;
      if (remaining <= 0) break;
      if (segment.text.length > remaining) {
        result.push({ type: "text", text: `${segment.text.slice(0, remaining)}…` });
        remaining = 0;
      } else {
        result.push(segment);
        remaining -= segment.text.length;
      }
    }
    return result;
  }

  /** Collapsed rows show a text-truncated preview; expanded rows show the full content. */
  function contentSegments(row: MysqlRecord, key: string): MessageContentSegment[] {
    const all = fullContentSegments(row);
    return isMessageDetailOpen(key) ? all : limitSegments(all, PREVIEW_TEXT_LIMIT);
  }

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
    memoryDialog,
    searchMemories,
    mutateMemory,
    submitMemoryDialog,
    image,
    searchImages,
    failedPreviews,
    onImageError,
    contentSegments,
    onMessageMediaError,
    toggleMessageDetail,
    hasMessageDetail,
    isMessageDetailOpen,
    pageTitle,
    userColumns,
    onUserRowClick,
    serviceTypeLabel,
    matchLabel,
    formatScore,
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
    deletingTurnId,
    deleteTurn,
    turnFinderVisible,
    contextFlowRef,
    focusTurn,
  };
}
