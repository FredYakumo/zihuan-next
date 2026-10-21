import { computed, ref, watch } from "vue";

import { useChat, type ChatEmit, type ChatProps } from "./useChat";

const HISTORY_COLLAPSED_KEY = "zihuan.chat.history-collapsed";
const SMALL_SCREEN_HISTORY_QUERY = "(max-width: 1180px)";

const TASK_TOOL_NAMES = new Set(["TaskCreate", "TaskUpdate", "TaskGet", "TaskList"]);

export interface WorkspaceDiffRow {
  path?: string;
  showPath?: boolean;
  before?: WorkspaceDiffCell;
  after?: WorkspaceDiffCell;
}

export interface WorkspaceDiffCell {
  kind: "added" | "removed" | "context";
  line: string;
  lineNumber?: number;
}

export interface WorkspaceDiffLine {
  path?: string;
  kind: "added" | "removed" | "context";
  line: string;
  before_line?: number;
  after_line?: number;
  hunk?: number;
}

export type MetricsMessage = {
  metrics?: {
    time_to_first_token_ms?: number;
    output_tokens_per_second?: number;
    prompt_tokens?: number;
    completion_tokens?: number;
    total_tokens?: number;
    cache_hit_rate?: number;
  };
};

export function useChatPage(props: ChatProps, emit: ChatEmit) {
  const chat = useChat(props, emit);

  const {
    sessions,
    activeSessionId,
    sending,
    isWorkspaceService,
    workspaceTasks,
    workspaceTaskInterrupted,
    messages,
    isCurrentCommandConfirmation,
    agentsMdEditorContent,
  } = chat;

  function resolveInitialHistoryCollapsed(): boolean {
    if (props.embedded) return false;
    const stored = localStorage.getItem(HISTORY_COLLAPSED_KEY);
    if (stored !== null) return stored === "1";
    return window.matchMedia(SMALL_SCREEN_HISTORY_QUERY).matches;
  }

  const historyCollapsed = ref(resolveInitialHistoryCollapsed());

  const activeSessionRunning = computed(() => {
    const session = sessions.value.find((item) => item.session_id === activeSessionId.value);
    return sending.value || session?.running_task_id != null || session?.task_status === "running";
  });
  const showWorkspaceTaskPanel = computed(
    () => isWorkspaceService.value && workspaceTasks.value.length > 0 && activeSessionRunning.value,
  );
  const showInterruptedWorkspaceTasks = computed(
    () => isWorkspaceService.value && workspaceTasks.value.some((task) => task.status !== "completed") && workspaceTaskInterrupted.value,
  );
  const lastAssistantMessageId = computed(
    () => [...messages.value].reverse().find((message) => message.role === "assistant")?.id ?? "",
  );

  function hasWorkspaceTaskToolCall(message: { toolCalls: Array<{ function: { name: string } }>; liveToolCalls?: Array<{ name: string }> }) {
    return message.toolCalls.some((call) => TASK_TOOL_NAMES.has(call.function.name)) ||
      message.liveToolCalls?.some((call) => TASK_TOOL_NAMES.has(call.name)) === true;
  }

  function toggleHistory() {
    historyCollapsed.value = !historyCollapsed.value;
  }

  watch(historyCollapsed, (collapsed) => {
    if (!props.embedded) {
      localStorage.setItem(HISTORY_COLLAPSED_KEY, collapsed ? "1" : "0");
    }
  });

  const currentCommandConfirmation = computed(() => {
    for (const message of [...messages.value].reverse()) {
      const call = [...(message.liveToolCalls ?? [])].reverse().find((item) => isCurrentCommandConfirmation(item) && !item.commandConfirmation?.decision);
      if (call?.commandConfirmation) return { call, ...call.commandConfirmation };
    }
    return null;
  });

  const agentsMdEditorRef = ref<HTMLTextAreaElement | null>(null);
  const agentsMdLineNumbersRef = ref<HTMLElement | null>(null);

  function workspaceDiffRows(diff: WorkspaceDiffLine[]): WorkspaceDiffRow[] {
    const rows: WorkspaceDiffRow[] = [];
    let previousPath: string | undefined;
    let start = 0;

    while (start < diff.length) {
      const hunk = diff[start].hunk ?? 0;
      let end = start;
      while (end < diff.length && (diff[end].hunk ?? 0) === hunk) {
        end += 1;
      }

      const hunkLines = diff.slice(start, end);
      let changeStart = 0;
      while (changeStart < hunkLines.length) {
        const change = hunkLines[changeStart];
        if (change.kind === "context") {
          rows.push({
            path: change.path,
            showPath: change.path !== previousPath,
            before: { kind: "context", line: change.line, lineNumber: change.before_line },
            after: { kind: "context", line: change.line, lineNumber: change.after_line },
          });
          previousPath = change.path;
          changeStart += 1;
          continue;
        }

        let changeEnd = changeStart;
        while (changeEnd < hunkLines.length && hunkLines[changeEnd].kind !== "context") {
          changeEnd += 1;
        }

        const removed = hunkLines.slice(changeStart, changeEnd).filter((line) => line.kind === "removed");
        const added = hunkLines.slice(changeStart, changeEnd).filter((line) => line.kind === "added");
        const rowCount = Math.max(removed.length, added.length);
        for (let index = 0; index < rowCount; index += 1) {
          const before = removed[index];
          const after = added[index];
          rows.push({
            path: before?.path ?? after?.path,
            showPath: (before?.path ?? after?.path) !== previousPath,
            before: before && { kind: before.kind, line: before.line, lineNumber: before.before_line },
            after: after && { kind: after.kind, line: after.line, lineNumber: after.after_line },
          });
          previousPath = before?.path ?? after?.path;
        }

        changeStart = changeEnd;
      }

      start = end;
    }

    return rows;
  }

  const agentsMdLineCount = computed(() => {
    const content = agentsMdEditorContent.value;
    if (!content) {
      return 1;
    }
    return content.split("\n").length;
  });

  function syncAgentsMdScroll() {
    const editor = agentsMdEditorRef.value;
    const gutter = agentsMdLineNumbersRef.value;
    if (editor && gutter) {
      gutter.scrollTop = editor.scrollTop;
    }
  }

  function hasMessageMetrics(message: MetricsMessage) {
    const metrics = message.metrics;
    return metrics?.time_to_first_token_ms != null
      || metrics?.output_tokens_per_second != null
      || metrics?.prompt_tokens != null
      || metrics?.completion_tokens != null
      || metrics?.total_tokens != null
      || metrics?.cache_hit_rate != null;
  }

  function formatDuration(milliseconds: number) {
    return `${(milliseconds / 1000).toFixed(milliseconds < 10_000 ? 2 : 1)}s`;
  }

  function formatOutputSpeed(tokensPerSecond: number) {
    return tokensPerSecond.toFixed(1);
  }

  function formatTokenCount(tokens: number) {
    return new Intl.NumberFormat().format(tokens);
  }

  function formatCacheHitRate(rate: number) {
    return `${(rate * 100).toFixed(1)}%`;
  }

  return {
    ...chat,
    HISTORY_COLLAPSED_KEY,
    SMALL_SCREEN_HISTORY_QUERY,
    resolveInitialHistoryCollapsed,
    historyCollapsed,
    TASK_TOOL_NAMES,
    activeSessionRunning,
    showWorkspaceTaskPanel,
    showInterruptedWorkspaceTasks,
    lastAssistantMessageId,
    hasWorkspaceTaskToolCall,
    toggleHistory,
    currentCommandConfirmation,
    agentsMdEditorRef,
    agentsMdLineNumbersRef,
    workspaceDiffRows,
    agentsMdLineCount,
    syncAgentsMdScroll,
    hasMessageMetrics,
    formatDuration,
    formatOutputSpeed,
    formatTokenCount,
    formatCacheHitRate,
  };
}
