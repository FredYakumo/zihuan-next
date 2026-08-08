import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import MarkdownIt from "markdown-it";

import {
  chat,
  fileIO,
  system,
  type ServiceWithRuntime,
  type ChatHistoryRecord,
  type ChatToolCall,
  type ChatSessionSummary,
  type ChatStreamEvent,
  type ChatMessagePart,
  type ChatMessageBranch,
  type LlmConfig,
} from "../../api/client";
import {
  formatTime,
  agentAvatarUrl,
  agentInitial,
  getAvatarDisplayUrl,
  CHAT_ELIGIBLE_SERVICE_TYPES,
} from "../model";

export interface ChatProps {
  agentId?: string;
  sessionId?: string;
  embedded?: boolean;
}

export type ChatEmit = (e: "update:sessionId", sessionId: string) => void;

export function useChat(props: ChatProps, emit: ChatEmit) {
type ChatRole = "user" | "assistant" | "tool";
type LiveToolCall = {
  call_id: string;
  name: string;
  arguments: unknown;
  result?: string;
  done: boolean;
};
type ChatMessage = {
  id: string;
  role: ChatRole;
  content: string;
  thinkingContent?: string;
  thinkingExpanded?: boolean;
  streaming?: boolean;
  timestamp?: string;
  toolCalls: ChatToolCall[];
  toolCallId?: string | null;
  linkedToolCall?: ChatToolCall | null;
  agentAvatarUrl?: string;
  agentName?: string;
  liveToolCalls?: LiveToolCall[];
  imageAttachments?: ChatImageAttachment[];
};
type ChatImageAttachment = {
  id: string;
  url: string;
  modelUrl?: string;
  key: string;
  mediaId: string;
  name: string;
  mimeType: string;
  uploading?: boolean;
  error?: string;
  localPreviewUrl?: string;
};
type EditingMessage = {
  messageId: string;
  content: string;
  imageAttachments: ChatImageAttachment[];
};
type SendMessageOptions = {
  attachments?: ChatImageAttachment[];
  isEdit?: boolean;
};

type ToolDetail = {
  messageId: string;
  toolCall: ChatToolCall;
  result: string;
};
type LineEditSpec = {
  start_line: number;
  end_line: number;
  replacement_lines: string[];
};
type SearchMatch = {
  path: string;
  line: number;
  content: string;
  context_before?: string[];
  context_after?: string[];
};
type ToolCallKind =
  | { type: "create_file"; filename: string; lineCount: number; content: string }
  | { type: "delete_file"; filename: string; lineCount: number | null }
  | { type: "edit_file"; filename: string; addedLines: number; removedLines: number; edits: LineEditSpec[] }
  | { type: "exec_cmd"; command: string; hasResult: boolean; stdout?: string; stderr?: string }
  | {
      type: "read_file";
      filename: string;
      startLine: number | null;
      endLine: number | null;
      totalLines: number | null;
      content: string;
    }
  | { type: "list_dir"; dirname: string; entries: Array<{ name: string; path: string; type: string }>; truncated: boolean }
  | { type: "grep" | "rg"; pattern: string; matches: SearchMatch[]; totalMatches: number; truncated: boolean }
  | { type: "generic"; name: string };



function basename(p: string): string {
  const seg = p.replace(/\\/g, "/").split("/");
  return seg[seg.length - 1] || p;
}

function safeParseJson<T>(raw: unknown): T | null {
  if (raw == null) return null;
  try {
    return typeof raw === "string" ? (JSON.parse(raw) as T) : (raw as T);
  } catch {
    return null;
  }
}

function classifyToolCall(name: string, arguments_: unknown, result?: string): ToolCallKind {
  if (name === "create_file") {
    const args = safeParseJson<{ path?: string; content?: string }>(arguments_);
    if (args?.path != null && args?.content != null) {
      return {
        type: "create_file",
        filename: basename(args.path),
        lineCount: args.content.split("\n").length,
        content: args.content,
      };
    }
  }
  if (name === "delete_file") {
    const args = safeParseJson<{ path?: string }>(arguments_);
    const res = safeParseJson<{ line_count?: number | null }>(result);
    if (args?.path != null) {
      return {
        type: "delete_file",
        filename: basename(args.path),
        lineCount: res?.line_count ?? null,
      };
    }
  }
  if (name === "edit_file") {
    const args = safeParseJson<{ path?: string; edits?: LineEditSpec[] }>(arguments_);
    if (args?.path != null && Array.isArray(args.edits)) {
      let addedLines = 0;
      let removedLines = 0;
      for (const edit of args.edits) {
        removedLines += Math.max(0, edit.end_line - edit.start_line + 1);
        addedLines += edit.replacement_lines.length;
      }
      return {
        type: "edit_file",
        filename: basename(args.path),
        addedLines,
        removedLines,
        edits: args.edits,
      };
    }
  }
  if (name === "exec_cmd") {
    const args = safeParseJson<{ command?: string }>(arguments_);
    const res = safeParseJson<{ stdout?: string; stderr?: string }>(result);
    const stdout = res?.stdout ?? "";
    const stderr = res?.stderr ?? "";
    const hasResult = stdout.length > 0 || stderr.length > 0;
    if (args?.command != null) {
      return {
        type: "exec_cmd",
        command: args.command,
        hasResult,
        stdout: hasResult ? stdout : undefined,
        stderr: hasResult ? stderr : undefined,
      };
    }
  }
  if (name === "read_file") {
    const args = safeParseJson<{ path?: string }>(arguments_);
    const res = safeParseJson<{
      content?: string;
      start_line?: number;
      end_line?: number;
      total_lines?: number;
    }>(result);
    if (args?.path != null) {
      return {
        type: "read_file",
        filename: basename(args.path),
        startLine: res?.start_line ?? null,
        endLine: res?.end_line ?? null,
        totalLines: res?.total_lines ?? null,
        content: res?.content ?? "",
      };
    }
  }
  if (name === "list_dir") {
    const args = safeParseJson<{ path?: string }>(arguments_);
    const res = safeParseJson<{
      entries?: Array<{ name?: string; path?: string; type?: string }>;
      truncated?: boolean;
    }>(result);
    if (args?.path != null) {
      return {
        type: "list_dir",
        dirname: basename(args.path),
        entries: (res?.entries ?? []).map((entry) => ({
          name: entry.name ?? "",
          path: entry.path ?? "",
          type: entry.type ?? "unknown",
        })),
        truncated: res?.truncated ?? false,
      };
    }
  }
  if (name === "grep" || name === "rg") {
    const args = safeParseJson<{ pattern?: string }>(arguments_);
    const res = safeParseJson<{
      pattern?: string;
      matches?: SearchMatch[];
      total_matches?: number;
      truncated?: boolean;
    }>(result);
    if (args?.pattern != null) {
      return {
        type: name,
        pattern: res?.pattern ?? args.pattern,
        matches: res?.matches ?? [],
        totalMatches: res?.total_matches ?? res?.matches?.length ?? 0,
        truncated: res?.truncated ?? false,
      };
    }
  }
  return { type: "generic", name };
}

type PendingNewConversationCommand = {
  passthroughText: string | null;
};
type PendingAskUser = {
  question: string;
  details?: string;
  placeholder?: string;
};
type StreamState = {
  assistantMessageId: string | null;
  toolMessageId: string | null;
  awaitingPostToolContent: boolean;
  pendingNewConversation: PendingNewConversationCommand | null;
  requestText: string;
};

const services = ref<ServiceWithRuntime[]>([]);
const servicesLoading = ref(false);
const sessions = ref<ChatSessionSummary[]>([]);
const activeSessionId = ref("");
const selectedServiceId = ref("");
const draftMessage = ref("");
const draftImageAttachments = ref<ChatImageAttachment[]>([]);
const imagePreviewAttachment = ref<ChatImageAttachment | null>(null);
const workspacePath = ref("");
const pickingDirectory = ref(false);
const sending = ref(false);
const chatErrorMessage = ref("");
const chatErrorDialogMessage = ref("");
const messagesContainer = ref<HTMLElement | null>(null);
const messages = ref<ChatMessage[]>([]);
const messageBranches = ref<ChatMessageBranch[]>([]);
const editingMessage = ref<EditingMessage | null>(null);
const copiedMessageId = ref("");
const activeToolCallId = ref("");
const expandedLiveToolCalls = ref(new Set<string>());
const llmModels = ref<LlmConfig[]>([]);
const selectedModelId = ref("");
const selectedThinkingType = ref<"" | "enabled" | "disabled">("");
const selectedReasoningEffort = ref<"" | "low" | "medium" | "high" | "max">("");
const openPicker = ref<"model" | "thinking" | "effort" | "settings" | null>(null);
const autoCollapseThinking = ref(true);
const stats = reactive({
  connections: 0,
  llm: 0,
  services: 0,
});
const markdown = new MarkdownIt({
  html: false,
  breaks: true,
  linkify: true,
});


const selectedService = computed(
  () => services.value.find((agent) => agent.config_id === selectedServiceId.value) ?? null,
);
const selectedServiceType = computed(() => selectedService.value?.agent_type?.type ?? "");
const isChatEligible = computed(() => CHAT_ELIGIBLE_SERVICE_TYPES.has(selectedServiceType.value));
const isWorkspaceService = computed(() => selectedServiceType.value === "workspace");
const groupedSessions = computed(() => {
  const groups = new Map<string, ChatSessionSummary[]>();
  for (const session of sessions.value) {
    const key = session.workspace_path ?? "__default__";
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(session);
  }
  const result: Array<{
    pathKey: string;
    path: string | null;
    label: string;
    sessions: ChatSessionSummary[];
  }> = [];
  for (const [key, items] of groups) {
    items.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
    const isDefault = key === "__default__";
    const segments = key.split(/[\\/]/).filter(Boolean);
    const label = isDefault ? "默认路径" : segments[segments.length - 1] || key;
    result.push({
      pathKey: key,
      path: isDefault ? null : key,
      label,
      sessions: items,
    });
  }
  result.sort((a, b) => b.sessions[0].updated_at.localeCompare(a.sessions[0].updated_at));
  return result;
});
const chatModels = computed(() => llmModels.value.filter((item) => item.model.type === "chat_llm"));
const selectedModelLlmConfig = computed(() => {
  const modelId = selectedModelId.value || defaultAgentModelId.value;
  const model = chatModels.value.find((m) => m.config_id === modelId);
  if (model?.model.type === "chat_llm") {
    return model.model.llm;
  }
  return null;
});
const supportsMultimodalInput = computed(() => selectedModelLlmConfig.value?.supports_multimodal_input === true);
const defaultAgentModelId = computed(() => {
  const agent = selectedService.value;
  if (!agent) {
    return "";
  }
  const agentType = agent.agent_type as Record<string, unknown>;
  return String(agentType.llm_ref_id ?? "");
});
const selectedModelLabel = computed(() => {
  if (!selectedModelId.value) {
    return "默认模型";
  }
  const model = chatModels.value.find((m) => m.config_id === selectedModelId.value);
  return model?.name ?? "默认模型";
});
const selectedThinkingLabel = computed(() => {
  const defaultType = selectedModelLlmConfig.value?.thinking_type;
  const defaultLabel = defaultType ? (defaultType === "enabled" ? "(启用)" : "(禁用)") : "";
  if (!selectedThinkingType.value) {
    return `默认${defaultLabel}`;
  }
  return selectedThinkingType.value === "enabled" ? "启用" : "禁用";
});
const selectedEffortLabel = computed(() => {
  const defaultEffort = selectedModelLlmConfig.value?.reasoning_effort;
  const defaultLabel = defaultEffort ? `(${defaultEffort})` : "";
  if (!selectedReasoningEffort.value) {
    return `默认${defaultLabel}`;
  }
  return selectedReasoningEffort.value;
});
const canSend = computed(() =>
  !!selectedService.value &&
  isChatEligible.value &&
  selectedService.value.runtime.status === "running" &&
  (draftMessage.value.trim().length > 0 || draftImageAttachments.value.length > 0) &&
  (supportsMultimodalInput.value || draftImageAttachments.value.length === 0) &&
  draftImageAttachments.value.every((attachment) => !attachment.uploading && !attachment.error),
);
const selectedAgentAvatarUrl = computed(() => agentAvatarUrl(selectedService.value));
const selectedAgentAvatarFallback = computed(() => {
  const name = selectedService.value?.name ?? "Bot";
  return agentInitial(name);
});
const pendingAskUser = ref<PendingAskUser | null>(null);
const askUserAnswer = ref("");
const canSubmitAskUser = computed(() =>
  isChatEligible.value &&
  isWorkspaceService.value &&
  !!pendingAskUser.value &&
  selectedService.value?.runtime.status === "running" &&
  askUserAnswer.value.trim().length > 0 &&
  !sending.value,
);

function parseNewConversationCommand(input: string): PendingNewConversationCommand | null {
  const match = input.trim().match(/^\/(new|clear|reset)(?:\s+([\s\S]*))?$/i);
  if (!match) {
    return null;
  }

  const passthroughText = (match[2] ?? "").trim();
  return {
    passthroughText: passthroughText.length > 0 ? passthroughText : null,
  };
}

function messageAvatarUrl(record: ChatHistoryRecord): string {
  if (record.agent_avatar_url) {
    return getAvatarDisplayUrl(record.agent_avatar_url);
  }
  const agent = services.value.find((a) => a.config_id === record.agent_id);
  return agentAvatarUrl(agent);
}

type MessageGroup = {
  id: string;
  role: ChatRole;
  messages: ChatMessage[];
  avatarUrl?: string;
  agentName?: string;
};

const messageGroups = computed(() => {
  const filtered = messages.value.filter((m) => m.role !== "tool");
  const groups: MessageGroup[] = [];
  let currentGroup: MessageGroup | null = null;

  for (const message of filtered) {
    if (currentGroup && currentGroup.role === message.role) {
      currentGroup.messages.push(message);
    } else {
      currentGroup = {
        id: `group-${message.id}`,
        role: message.role,
        messages: [message],
        avatarUrl:
          message.role === "assistant"
            ? message.agentAvatarUrl || selectedAgentAvatarUrl.value || undefined
            : undefined,
        agentName: message.agentName || selectedService.value?.name,
      };
      groups.push(currentGroup);
    }
  }
  return groups;
});

const messageBranchMap = computed(() =>
  new Map(messageBranches.value.map((branch) => [branch.message_id, branch])),
);

const activeToolDetail = computed<ToolDetail | null>(() => {
  if (!activeToolCallId.value) {
    return null;
  }
  for (const message of messages.value) {
    const toolCall = message.toolCalls.find((item) => item.id === activeToolCallId.value);
    if (!toolCall) {
      continue;
    }
    const resultMessage = messages.value.find(
      (item) => item.role === "tool" && item.toolCallId === toolCall.id,
    );
    return {
      messageId: message.id,
      toolCall,
      result: resultMessage?.content ?? "",
    };
  }
  return null;
});

function readableAgentType(type: string): string {
  if (type === "http_stream") {
    return "HTTP stream service";
  }
  if (type === "workspace") {
    return "Workspace Agent Service";
  }
  return "QQ Chat Agent Service";
}

function imageAttachmentToPart(attachment: ChatImageAttachment): ChatMessagePart {
  return {
    type: "image",
    media: {
      media_id: attachment.mediaId,
      source: "upload",
      original_source: attachment.modelUrl ?? attachment.url,
      rustfs_path: "",
      name: attachment.name,
      mime_type: attachment.mimeType,
    },
  };
}

function isProviderImageUrl(url: string): boolean {
  return url.startsWith("http://") || url.startsWith("https://") || url.startsWith("data:image/");
}

function imageAttachmentsFromParts(parts: ChatMessagePart[] | undefined): ChatImageAttachment[] {
  return (parts ?? []).flatMap((part, index) => {
    if (part.type !== "image" || !part.media) {
      return [];
    }
    const providerUrl = [part.media.original_source, part.media.rustfs_path].find(isProviderImageUrl);
    const url = part.media.original_source || part.media.rustfs_path;
    if (!url) {
      return [];
    }
    return [{
      id: part.media.media_id || `history-image-${index}-${url}`,
      url,
      key: part.media.media_id,
      mediaId: part.media.media_id,
      name: part.media.name || "图片",
      mimeType: part.media.mime_type || "image/*",
      modelUrl: providerUrl,
    }];
  });
}

function messageParts(content: string, attachments: ChatImageAttachment[] | undefined): ChatMessagePart[] | undefined {
  if (!attachments?.length) {
    return undefined;
  }
  const parts: ChatMessagePart[] = [];
  if (content) {
    parts.push({ type: "text", text: content });
  }
  parts.push(...attachments.map(imageAttachmentToPart));
  return parts;
}

function toApiMessages() {
  return messages.value
    .filter(
      (item) =>
        item.content.trim().length > 0 ||
        item.imageAttachments?.length ||
        item.toolCalls.length > 0 ||
        !!item.toolCallId,
    )
    .map((item) => ({
      role: item.role,
      content: item.content,
      parts: messageParts(item.content, item.imageAttachments),
      tool_calls: item.toolCalls.length > 0 ? item.toolCalls : undefined,
      tool_call_id: item.toolCallId ?? undefined,
    }));
}

function applyHistory(records: ChatHistoryRecord[]) {
  const mapped: ChatMessage[] = records
    .filter((item) => item.role === "user" || item.role === "assistant" || item.role === "tool")
    .map((item) => ({
      id: item.message_id,
      role: item.role as ChatRole,
      content: item.content,
      imageAttachments: imageAttachmentsFromParts(item.parts),
      thinkingContent: item.reasoning_content ?? undefined,
      thinkingExpanded: !autoCollapseThinking.value && !!item.reasoning_content,
      timestamp: item.timestamp,
      toolCalls: item.tool_calls ?? [],
      toolCallId: item.tool_call_id ?? null,
      linkedToolCall: null,
      agentAvatarUrl: messageAvatarUrl(item) || undefined,
      agentName: item.agent_name || undefined,
    }));
  const toolCallMap = new Map<string, ChatToolCall>();
  for (const message of mapped) {
    for (const toolCall of message.toolCalls) {
      toolCallMap.set(toolCall.id, toolCall);
    }
  }
  for (const message of mapped) {
    if (message.role === "tool" && message.toolCallId) {
      message.linkedToolCall = toolCallMap.get(message.toolCallId) ?? null;
    }
  }
  messages.value = mapped;
  if (activeToolCallId.value) {
    const stillExists = mapped.some((message) =>
      message.toolCalls.some((toolCall) => toolCall.id === activeToolCallId.value),
    );
    if (!stillExists) {
      activeToolCallId.value = "";
    }
  }
  scrollToBottom();
}

function openToolDetail(messageId: string, toolCallId: string) {
  const message = messages.value.find((item) => item.id === messageId);
  if (!message || !message.toolCalls.some((item) => item.id === toolCallId)) {
    return;
  }
  activeToolCallId.value = activeToolCallId.value === toolCallId ? "" : toolCallId;
}

function closeToolDetail() {
  activeToolCallId.value = "";
}

function getToolResultText(toolCallId: string): string | undefined {
  const resultMessage = messages.value.find(
    (item) => item.role === "tool" && item.toolCallId === toolCallId,
  );
  return resultMessage?.content || undefined;
}

type ToolPreviewData = {
  kind: ToolCallKind;
  toolCallId: string;
};

const toolPreviewState = ref<ToolPreviewData | null>(null);

function openToolPreview(kind: ToolCallKind) {
  toolPreviewState.value = { kind, toolCallId: "" };
}

function closeToolPreview() {
  toolPreviewState.value = null;
}

function handleToolPreviewKeydown(e: KeyboardEvent) {
  if (e.key === "Escape" && toolPreviewState.value) {
    closeToolPreview();
  }
}

function editHunks(edits: LineEditSpec[]): { startLine: number; removed: string[]; added: string[] }[] {
  const sorted = [...edits].sort((a, b) => a.start_line - b.start_line);
  const hunks: { startLine: number; removed: string[]; added: string[] }[] = [];
  for (const edit of sorted) {
    const removedCount = Math.max(0, edit.end_line - edit.start_line + 1);
    hunks.push({
      startLine: edit.start_line,
      removed: Array.from({ length: removedCount }, (_, i) => `L${edit.start_line + i}`),
      added: edit.replacement_lines.map(
        (line, i) => `L${edit.start_line + i}` + (line.length > 0 ? ": " + line : ""),
      ),
    });
  }
  return hunks;
}

function toggleLiveToolCall(callId: string) {
  if (expandedLiveToolCalls.value.has(callId)) {
    expandedLiveToolCalls.value.delete(callId);
  } else {
    expandedLiveToolCalls.value.add(callId);
  }
  expandedLiveToolCalls.value = new Set(expandedLiveToolCalls.value);
}

function formatToolPayload(payload: unknown): string {
  if (payload == null) {
    return "null";
  }
  if (typeof payload === "string") {
    return payload;
  }
  try {
    return JSON.stringify(payload, null, 2);
  } catch {
    return String(payload);
  }
}

function formatChatTime(timestamp?: string): string {
  if (!timestamp) {
    return "";
  }
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return timestamp;
  }
  return date.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function renderMessageContent(content: string, streaming = false): string {
  const text = content || (streaming ? "..." : "");
  return markdown.render(text);
}

function scrollToBottom() {
  nextTick(() => {
    if (messagesContainer.value) {
      messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight;
    }
  });
}

function clearChatError() {
  chatErrorMessage.value = "";
}

function showChatError(message: string) {
  chatErrorMessage.value = message;
  chatErrorDialogMessage.value = message;
}

function createStreamingAssistantMessage(): ChatMessage {
  return {
    id: `local-assistant-${crypto.randomUUID()}`,
    role: "assistant",
    content: "",
    streaming: true,
    timestamp: new Date().toISOString(),
    toolCalls: [],
    toolCallId: null,
    linkedToolCall: null,
  };
}

function hasMessageContent(message: ChatMessage): boolean {
  return message.content.trim().length > 0 || (message.thinkingContent?.trim().length ?? 0) > 0;
}

function appendStreamingAssistantMessage(streamState: StreamState): ChatMessage {
  const current = streamState.assistantMessageId
    ? messages.value.find((item) => item.id === streamState.assistantMessageId)
    : undefined;
  if (current) {
    current.streaming = false;
  }

  const message = createStreamingAssistantMessage();
  messages.value.push(message);
  streamState.assistantMessageId = message.id;
  return message;
}

function getStreamingContentTarget(streamState: StreamState): ChatMessage | undefined {
  if (streamState.awaitingPostToolContent) {
    streamState.awaitingPostToolContent = false;
    return appendStreamingAssistantMessage(streamState);
  }

  return streamState.assistantMessageId
    ? messages.value.find((item) => item.id === streamState.assistantMessageId)
    : undefined;
}

function closeChatErrorDialog() {
  chatErrorDialogMessage.value = "";
}

function handleTextareaKeydown(event: KeyboardEvent) {
  if (event.key !== "Enter") {
    return;
  }
  if (event.shiftKey) {
    return;
  }
  event.preventDefault();
  sendMessage();
}

function handleTextareaPaste(event: ClipboardEvent) {
  const files = Array.from(event.clipboardData?.items ?? [])
    .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
    .map((item) => item.getAsFile())
    .filter((file): file is File => file != null);
  if (files.length === 0) {
    return;
  }
  event.preventDefault();
  if (!supportsMultimodalInput.value) {
    showChatError("当前模型不支持多模态输入，无法添加图片。");
    return;
  }
  addImageFilesTo(draftImageAttachments.value, files);
}

function handleImageFileSelection(event: Event) {
  const input = event.target as HTMLInputElement;
  if (input.files) {
    addImageFilesTo(draftImageAttachments.value, Array.from(input.files));
  }
  input.value = "";
}

function handleEditImageFileSelection(event: Event) {
  const input = event.target as HTMLInputElement;
  if (input.files && editingMessage.value) {
    addImageFilesTo(editingMessage.value.imageAttachments, Array.from(input.files));
  }
  input.value = "";
}

function addImageFilesTo(target: ChatImageAttachment[], files: File[]) {
  if (!supportsMultimodalInput.value) {
    showChatError("当前模型不支持多模态输入，无法添加图片。");
    return;
  }
  for (const file of files) {
    if (!file.type.startsWith("image/")) {
      continue;
    }
    const attachment: ChatImageAttachment = {
      id: crypto.randomUUID(),
      url: URL.createObjectURL(file),
      key: "",
      mediaId: "",
      name: file.name || "图片",
      mimeType: file.type,
      uploading: true,
    };
    attachment.localPreviewUrl = attachment.url;
    target.push(attachment);
    void fileIO.uploadImage(file)
      .then(async (uploaded) => {
        const modelUrl = isProviderImageUrl(uploaded.url) ? uploaded.url : await readImageAsDataUrl(file);
        const current = target.find((item) => item.id === attachment.id);
        if (!current) {
          return;
        }
        if (current.localPreviewUrl) {
          URL.revokeObjectURL(current.localPreviewUrl);
        }
        current.url = uploaded.url;
        current.modelUrl = modelUrl;
        current.key = uploaded.key;
        current.mediaId = uploaded.media_id;
        current.name = uploaded.name;
        current.uploading = false;
        current.localPreviewUrl = undefined;
      })
      .catch((error: Error) => {
        const current = target.find((item) => item.id === attachment.id);
        if (current) {
          current.uploading = false;
          current.error = `上传失败: ${error.message}`;
          showChatError(current.error);
        }
      });
  }
}

function readImageAsDataUrl(file: File): Promise<string> {
  return readBlobAsDataUrl(file);
}

function readBlobAsDataUrl(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("读取图片失败"));
    reader.readAsDataURL(blob);
  });
}

async function resolveAttachmentModelUrl(attachment: ChatImageAttachment): Promise<void> {
  if (attachment.modelUrl && isProviderImageUrl(attachment.modelUrl)) {
    return;
  }
  if (isProviderImageUrl(attachment.url)) {
    attachment.modelUrl = attachment.url;
    return;
  }

  const response = await fetch(attachment.url);
  if (!response.ok) {
    throw new Error(`无法读取历史图片 ${attachment.name}: HTTP ${response.status}`);
  }
  const blob = await response.blob();
  if (!blob.type.startsWith("image/")) {
    throw new Error(`历史附件 ${attachment.name} 不是有效图片`);
  }
  attachment.modelUrl = await readBlobAsDataUrl(blob);
}

async function resolveConversationImageUrls(extraAttachments: ChatImageAttachment[] = []): Promise<void> {
  const attachments = messages.value.flatMap((message) => message.imageAttachments ?? []);
  attachments.push(...draftImageAttachments.value);
  attachments.push(...extraAttachments);
  await Promise.all(attachments.map(resolveAttachmentModelUrl));
}

function removeDraftImageAttachment(id: string) {
  removeImageAttachment(draftImageAttachments.value, id);
}

function removeEditingImageAttachment(id: string) {
  if (editingMessage.value) {
    removeImageAttachment(editingMessage.value.imageAttachments, id);
  }
}

function removeImageAttachment(target: ChatImageAttachment[], id: string) {
  const index = target.findIndex((attachment) => attachment.id === id);
  if (index < 0) {
    return;
  }
  const [attachment] = target.splice(index, 1);
  if (attachment.localPreviewUrl) {
    URL.revokeObjectURL(attachment.localPreviewUrl);
  }
}

function openImagePreview(attachment: ChatImageAttachment) {
  imagePreviewAttachment.value = attachment;
}

function closeImagePreview() {
  imagePreviewAttachment.value = null;
}

function handleImagePreviewKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    closeImagePreview();
  }
}

function handleDocumentKeydown(event: KeyboardEvent) {
  handleToolPreviewKeydown(event);
  handleImagePreviewKeydown(event);
}

function toggleAutoCollapseThinking() {
  autoCollapseThinking.value = !autoCollapseThinking.value;
  openPicker.value = null;
}

function clearPendingAskUser() {
  pendingAskUser.value = null;
  askUserAnswer.value = "";
}

function pruneFailedAssistantPlaceholder(assistantMessageId: string | null) {
  if (!assistantMessageId) {
    return;
  }
  const index = messages.value.findIndex((item) => item.id === assistantMessageId);
  if (index < 0) {
    return;
  }

  const message = messages.value[index];
  message.streaming = false;

  const hasVisibleContent =
    message.content.trim().length > 0 || (message.thinkingContent?.trim().length ?? 0) > 0;
  const hasToolActivity =
    (message.liveToolCalls?.length ?? 0) > 0 || message.toolCalls.length > 0;
  if (!hasVisibleContent && !hasToolActivity) {
    messages.value.splice(index, 1);
  }
}

function applyInferenceFailure(streamState: StreamState, errorMessage: string) {
  pruneFailedAssistantPlaceholder(streamState.assistantMessageId);
  showChatError(`推理失败: ${errorMessage}`);
  if (!draftMessage.value.trim()) {
    draftMessage.value = streamState.requestText;
  }
}

async function reloadSessions() {
  const result = await chat.listSessions(selectedServiceId.value || undefined);
  sessions.value = result.sessions;
}

async function openSession(sessionId: string) {
  activeSessionId.value = sessionId;
  emit("update:sessionId", sessionId);
  clearChatError();
  clearPendingAskUser();
  const result = await chat.getSessionMessages(sessionId);
  const firstRecord = result.messages[0];
  if (firstRecord?.agent_id && services.value.some((a) => a.config_id === firstRecord.agent_id)) {
    selectedServiceId.value = firstRecord.agent_id;
  }
  workspacePath.value =
    result.messages[result.messages.length - 1]?.workspace_path ??
    firstRecord?.workspace_path ??
    "";
  const latestRecord = result.messages[result.messages.length - 1];
  if (latestRecord?.pending_ask_user?.question) {
    pendingAskUser.value = {
      question: latestRecord.pending_ask_user.question,
      details: latestRecord.pending_ask_user.details ?? undefined,
      placeholder: latestRecord.pending_ask_user.placeholder ?? undefined,
    };
  }
  applyHistory(result.messages);
  messageBranches.value = result.branches;
  editingMessage.value = null;
}

async function copyMessage(message: ChatMessage): Promise<void> {
  try {
    await navigator.clipboard.writeText(message.content);
    copiedMessageId.value = message.id;
    window.setTimeout(() => {
      if (copiedMessageId.value === message.id) {
        copiedMessageId.value = "";
      }
    }, 1600);
  } catch (error) {
    showChatError(`复制失败: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function startEditingMessage(message: ChatMessage) {
  if (sending.value || message.role !== "user" || message.id.startsWith("local-")) {
    return;
  }
  editingMessage.value = {
    messageId: message.id,
    content: message.content,
    imageAttachments: (message.imageAttachments ?? []).map((attachment) => ({ ...attachment })),
  };
}

function cancelEditingMessage() {
  editingMessage.value = null;
}

async function switchMessageBranch(messageId: string, direction: -1 | 1) {
  const branch = messageBranchMap.value.get(messageId);
  if (!branch || sending.value) {
    return;
  }
  const target = branch.versions[branch.current_index + direction];
  if (target) {
    await openSession(target.session_id);
  }
}

async function submitEditingMessage() {
  const editing = editingMessage.value;
  if (!editing || sending.value || !activeSessionId.value) {
    return;
  }
  const content = editing.content.trim();
  const attachments = editing.imageAttachments;
  if (!content && attachments.length === 0) {
    return;
  }
  if (!supportsMultimodalInput.value && attachments.length > 0) {
    showChatError("当前模型不支持多模态输入，无法发送图片。");
    return;
  }
  if (attachments.some((attachment) => attachment.uploading || attachment.error)) {
    showChatError("图片尚未准备完成，无法发送编辑后的消息。");
    return;
  }

  try {
    const forked = await chat.forkSession(activeSessionId.value, editing.messageId);
    await openSession(forked.session_id);
    editingMessage.value = null;
    await sendMessageWithText(content, false, { attachments, isEdit: true });
  } catch (error) {
    showChatError(`创建对话分支失败: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function pickDirectory() {
  pickingDirectory.value = true;
  try {
    const result = await system.selectDirectory();
    if (result.path) {
      workspacePath.value = result.path;
    }
  } catch (error) {
    chatErrorMessage.value = `选择目录失败: ${(error as Error).message}`;
  } finally {
    pickingDirectory.value = false;
  }
}

function startNewSession() {
  activeSessionId.value = "";
  emit("update:sessionId", "");
  messages.value = [];
  activeToolCallId.value = "";
  expandedLiveToolCalls.value = new Set();
  messageBranches.value = [];
  editingMessage.value = null;
  clearChatError();
  clearPendingAskUser();
}

function selectModel(id: string) {
  selectedModelId.value = id;
  openPicker.value = null;
}

function selectThinkingType(value: "" | "enabled" | "disabled") {
  selectedThinkingType.value = value;
  openPicker.value = null;
}

function selectReasoningEffort(value: "" | "low" | "medium" | "high" | "max") {
  selectedReasoningEffort.value = value;
  openPicker.value = null;
}

function closePickersOnClickOutside(event: MouseEvent) {
  const target = event.target as HTMLElement;
  if (!target.closest(".model-picker") && !target.closest(".model-settings")) {
    openPicker.value = null;
  }
}

watch(selectedServiceId, async () => {
  await reloadSessions();
  startNewSession();
  selectedModelId.value = defaultAgentModelId.value;
  selectedThinkingType.value = "";
  selectedReasoningEffort.value = "";
  if (!isWorkspaceService.value) {
    workspacePath.value = "";
  }
  if (!isChatEligible.value) {
    clearPendingAskUser();
  }
});

watch(selectedModelId, () => {
  selectedThinkingType.value = "";
  selectedReasoningEffort.value = "";
});

async function removeSession(sessionId: string) {
  if (!confirm("确定要删除该会话吗？此操作不可恢复。")) {
    return;
  }
  try {
    await chat.deleteSession(sessionId);
    sessions.value = sessions.value.filter((s) => s.session_id !== sessionId);
    if (activeSessionId.value === sessionId) {
      startNewSession();
    }
  } catch (error) {
    alert(`删除失败: ${(error as Error).message}`);
  }
}

function applyStreamEvent(event: ChatStreamEvent, streamState: StreamState) {
  if (event.type === "error") {
    applyInferenceFailure(streamState, event.error ?? "未知错误");
    return;
  }

  if (event.type === "ask_user" && event.question) {
    pendingAskUser.value = {
      question: event.question,
      details: event.details ?? undefined,
      placeholder: event.placeholder ?? undefined,
    };
    askUserAnswer.value = "";
    return;
  }

  if (event.type === "start") {
    if (streamState.pendingNewConversation) {
      startNewSession();
      if (event.session_id) {
        activeSessionId.value = event.session_id;
        emit("update:sessionId", event.session_id);
      }

      const passthroughText = streamState.pendingNewConversation.passthroughText;
      if (passthroughText) {
        messages.value.push({
          id: `local-user-${crypto.randomUUID()}`,
          role: "user",
          content: passthroughText,
          timestamp: new Date().toISOString(),
          toolCalls: [],
          toolCallId: null,
          linkedToolCall: null,
        });

        const assistantMessageId = event.message_id ?? `local-assistant-${crypto.randomUUID()}`;
        messages.value.push({
          id: assistantMessageId,
          role: "assistant",
          content: "",
          streaming: true,
          timestamp: new Date().toISOString(),
          toolCalls: [],
          toolCallId: null,
          linkedToolCall: null,
        });
        streamState.assistantMessageId = assistantMessageId;
      } else {
        streamState.assistantMessageId = event.message_id ?? null;
      }

      streamState.pendingNewConversation = null;
      scrollToBottom();
      return;
    }

    if (event.session_id) {
      activeSessionId.value = event.session_id;
      emit("update:sessionId", event.session_id);
    }
    if (event.message_id) {
      const currentAssistantId = streamState.assistantMessageId;
      const message = currentAssistantId
        ? messages.value.find((item) => item.id === currentAssistantId || item.id === event.message_id)
        : undefined;
      if (message) {
        message.id = event.message_id;
      }
      streamState.assistantMessageId = event.message_id;
    }
  }

  if (event.type === "delta") {
    const message = getStreamingContentTarget(streamState);
    if (message) {
      message.content += event.token ?? "";
      message.streaming = true;
      scrollToBottom();
    }
  }

  if (event.type === "thinking_delta") {
    const message = getStreamingContentTarget(streamState);
    if (message) {
      if (!message.thinkingContent) {
        message.thinkingContent = "";
        message.thinkingExpanded = true;
      }
      message.thinkingContent += event.token ?? "";
      message.streaming = true;
      scrollToBottom();
    }
  }

  if (event.type === "done") {
    const targetId = streamState.assistantMessageId || event.message_id;
    if (!targetId) {
      return;
    }
    const message = messages.value.find((item) => item.id === targetId);
    if (message) {
      message.streaming = false;
      if (autoCollapseThinking.value && message.thinkingContent) {
        message.thinkingExpanded = false;
      }
    }
  }

  if (event.type === "tool_call_start" && event.call_id && event.name) {
    let message = streamState.toolMessageId
      ? messages.value.find((item) => item.id === streamState.toolMessageId)
      : undefined;
    if (!message || streamState.assistantMessageId !== message.id) {
      const current = streamState.assistantMessageId
        ? messages.value.find((item) => item.id === streamState.assistantMessageId)
        : undefined;
      if (current && !hasMessageContent(current) && current.toolCalls.length === 0 && !current.liveToolCalls?.length) {
        messages.value = messages.value.filter((item) => item.id !== current.id);
      }
      message = appendStreamingAssistantMessage(streamState);
      streamState.toolMessageId = message.id;
    }

    streamState.assistantMessageId = message.id;
    streamState.awaitingPostToolContent = true;
    if (!message.liveToolCalls) {
      message.liveToolCalls = [];
    }
    const existing = message.liveToolCalls.find((item) => item.call_id === event.call_id);
    if (existing) {
      existing.name = event.name;
      existing.arguments = event.arguments;
    } else {
      message.liveToolCalls.push({
        call_id: event.call_id,
        name: event.name,
        arguments: event.arguments,
        done: false,
      });
    }
    scrollToBottom();
  }

  if (event.type === "tool_call_result" && event.call_id) {
    const targetId = streamState.toolMessageId || streamState.assistantMessageId || event.message_id;
    const message = targetId ? messages.value.find((item) => item.id === targetId) : undefined;
    if (message) {
      if (!message.liveToolCalls) {
        message.liveToolCalls = [];
      }
      const liveCall = message.liveToolCalls.find((item) => item.call_id === event.call_id);
      if (liveCall) {
        liveCall.result = event.result ?? "";
        liveCall.done = true;
      } else {
        message.liveToolCalls.push({
          call_id: event.call_id,
          name: event.name ?? "unknown",
          arguments: event.arguments ?? {},
          result: event.result ?? "",
          done: true,
        });
      }
      scrollToBottom();
    }
  }
}

async function sendMessage() {
  await sendMessageWithText(draftMessage.value, false);
}

async function submitAskUserAnswer() {
  await sendMessageWithText(askUserAnswer.value, true);
}

async function sendMessageWithText(rawInput: string, fromAskUser: boolean, options: SendMessageOptions = {}) {
  if (sending.value) {
    return;
  }

  const userText = rawInput.trim();
  const sentAttachments = fromAskUser ? [] : options.attachments ?? draftImageAttachments.value;
  if (!fromAskUser && sentAttachments.length > 0 && !supportsMultimodalInput.value) {
    showChatError("当前模型不支持多模态输入，无法发送图片。");
    return;
  }
  if (!userText && (!fromAskUser && sentAttachments.length === 0)) {
    return;
  }
  if (!selectedService.value || selectedService.value.runtime.status !== "running") {
    return;
  }
  if (!fromAskUser && !options.isEdit && !canSend.value) {
    return;
  }
  if (fromAskUser && !canSubmitAskUser.value) {
    return;
  }
  clearChatError();
  sending.value = true;
  try {
    await resolveConversationImageUrls(options.attachments);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    showChatError(`图片准备失败: ${message}`);
    sending.value = false;
    return;
  }
  const pendingNewConversation = options.isEdit ? null : parseNewConversationCommand(userText);
  const requestMessages = [
    ...toApiMessages(),
    {
      role: "user",
      content: userText,
      parts: fromAskUser ? undefined : messageParts(userText, sentAttachments),
    },
  ];

  if (fromAskUser) {
    askUserAnswer.value = "";
  } else if (!options.attachments) {
    draftMessage.value = "";
    draftImageAttachments.value = [];
  }
  const streamState: StreamState = {
    assistantMessageId: null,
    toolMessageId: null,
    awaitingPostToolContent: false,
    pendingNewConversation,
    requestText: userText,
  };

  if (!pendingNewConversation) {
    const userMessage = {
      id: `local-user-${crypto.randomUUID()}`,
      role: "user" as const,
      content: userText,
      timestamp: new Date().toISOString(),
      toolCalls: [],
      toolCallId: null,
      linkedToolCall: null,
      imageAttachments: sentAttachments,
    };
    messages.value.push(userMessage);

    const assistantTempId = `local-assistant-${crypto.randomUUID()}`;
    messages.value.push({
      id: assistantTempId,
      role: "assistant",
      content: "",
      streaming: true,
      timestamp: new Date().toISOString(),
      toolCalls: [],
      toolCallId: null,
      linkedToolCall: null,
    });
    streamState.assistantMessageId = assistantTempId;
    scrollToBottom();
  }

  try {
    await chat.stream(
      {
        agent_id: selectedServiceId.value,
        session_id: activeSessionId.value || null,
        stream: true,
        model_config_id: selectedModelId.value || null,
        thinking_type: selectedThinkingType.value || null,
        reasoning_effort: selectedReasoningEffort.value || null,
        workspace_path: isWorkspaceService.value ? workspacePath.value.trim() || null : null,
        messages: requestMessages,
      },
      (event) => applyStreamEvent(event, streamState),
    );
    await reloadSessions();
    if (activeSessionId.value) {
      await openSession(activeSessionId.value);
    }
  } catch (error) {
    applyInferenceFailure(streamState, (error as Error).message);
  } finally {
    sending.value = false;
  }
}

async function load() {
  servicesLoading.value = true;
  try {
    const [connections, llm, loadedAgents] = await Promise.all([
      system.connections.list(),
      system.llm.list(),
      system.services.list(),
    ]);
    stats.connections = connections.length;
    stats.llm = llm.length;
    stats.services = loadedAgents.length;
    services.value = loadedAgents;
    llmModels.value = llm;

    const eligible = loadedAgents.filter((agent) => CHAT_ELIGIBLE_SERVICE_TYPES.has(agent.agent_type.type));
    const requestedAgent = props.agentId
      ? loadedAgents.find((agent) => agent.config_id === props.agentId)
      : null;

    if (requestedAgent && CHAT_ELIGIBLE_SERVICE_TYPES.has(requestedAgent.agent_type.type)) {
      selectedServiceId.value = requestedAgent.config_id;
    } else if (
      !selectedServiceId.value ||
      !loadedAgents.some((agent) => agent.config_id === selectedServiceId.value)
    ) {
      selectedServiceId.value = eligible[0]?.config_id ?? loadedAgents[0]?.config_id ?? "";
    }
    selectedModelId.value = defaultAgentModelId.value;
  } finally {
    servicesLoading.value = false;
  }

  await reloadSessions();

  if (props.sessionId) {
    await openSession(props.sessionId).catch((error) => {
      console.warn("Failed to open requested session:", error);
    });
  }
}

onMounted(() => {
  load().catch((error) => {
    console.error(error);
    alert(`Chat 加载失败: ${(error as Error).message}`);
  });
  document.addEventListener("click", closePickersOnClickOutside);
  document.addEventListener("keydown", handleDocumentKeydown);
});

onUnmounted(() => {
  document.removeEventListener("click", closePickersOnClickOutside);
  document.removeEventListener("keydown", handleDocumentKeydown);
});

  return {
    services,
    servicesLoading,
    sessions,
    activeSessionId,
    selectedServiceId,
    draftMessage,
    draftImageAttachments,
    imagePreviewAttachment,
    workspacePath,
    pickingDirectory,
    sending,
    chatErrorMessage,
    chatErrorDialogMessage,
    messagesContainer,
    messages,
    messageBranchMap,
    editingMessage,
    copiedMessageId,
    activeToolCallId,
    expandedLiveToolCalls,
    llmModels,
    selectedModelId,
    selectedThinkingType,
    selectedReasoningEffort,
    openPicker,
    autoCollapseThinking,
    stats,
    selectedService,
    selectedServiceType,
    isChatEligible,
    isWorkspaceService,
    groupedSessions,
    chatModels,
    selectedModelLlmConfig,
    supportsMultimodalInput,
    defaultAgentModelId,
    selectedModelLabel,
    selectedThinkingLabel,
    selectedEffortLabel,
    canSend,
    selectedAgentAvatarUrl,
    selectedAgentAvatarFallback,
    pendingAskUser,
    askUserAnswer,
    canSubmitAskUser,
    messageGroups,
    activeToolDetail,
    toolPreviewState,
    basename,
    safeParseJson,
    classifyToolCall,
    parseNewConversationCommand,
    messageAvatarUrl,
    readableAgentType,
    toApiMessages,
    applyHistory,
    openToolDetail,
    closeToolDetail,
    getToolResultText,
    openToolPreview,
    closeToolPreview,
    handleToolPreviewKeydown,
    editHunks,
    toggleLiveToolCall,
    formatToolPayload,
    formatChatTime,
    renderMessageContent,
    scrollToBottom,
    clearChatError,
    closeChatErrorDialog,
    handleTextareaKeydown,
    handleTextareaPaste,
    handleImageFileSelection,
    handleEditImageFileSelection,
    removeDraftImageAttachment,
    removeEditingImageAttachment,
    openImagePreview,
    closeImagePreview,
    handleImagePreviewKeydown,
    toggleAutoCollapseThinking,
    clearPendingAskUser,
    pruneFailedAssistantPlaceholder,
    applyInferenceFailure,
    reloadSessions,
    openSession,
    copyMessage,
    startEditingMessage,
    cancelEditingMessage,
    submitEditingMessage,
    switchMessageBranch,
    pickDirectory,
    startNewSession,
    selectModel,
    selectThinkingType,
    selectReasoningEffort,
    closePickersOnClickOutside,
    removeSession,
    applyStreamEvent,
    sendMessage,
    submitAskUserAnswer,
    sendMessageWithText,
    load,
    formatTime,
    agentAvatarUrl,
    agentInitial,
    getAvatarDisplayUrl,
    CHAT_ELIGIBLE_SERVICE_TYPES,
  };
}

export type UseChatReturn = ReturnType<typeof useChat>;
