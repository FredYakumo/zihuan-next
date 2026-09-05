import { toRef, type Ref } from "vue";

export interface ToolCallBadgeProps {
  kind: ToolCallKind;
  loading?: boolean;
}

export type ToolCallBadgeEmit = (e: "click") => void;

type SearchMatch = {
  path: string;
  line: number;
  content: string;
  context_before?: string[];
  context_after?: string[];
};
type WebSearchResult = { title: string; url: string; content: string; score: number | null };

type ToolCallKind =
  | { type: "create_file"; filename: string; lineCount: number; content: string }
  | { type: "delete_file"; filename: string; lineCount: number | null }
  | {
      type: "edit_file";
      filename: string;
      addedLines: number;
      removedLines: number;
      patch: string;
    }
  | { type: "copy_file" | "move_file"; src: string; dest: string; overwritten: boolean }
  | { type: "file_info"; filename: string; metadata: Record<string, unknown> }
  | { type: "find_files"; pattern: string; matches: Array<{ name: string; path: string; type: string }>; truncated: boolean }
  | { type: "git_status"; branch: string; changes: Array<{ status: string; path: string }>; truncated: boolean }
  | { type: "exec_cmd"; command: string; hasResult: boolean; stdout?: string; stderr?: string; shell?: string; exitCode?: number | null; truncated?: boolean }
  | {
      type: "read_file";
      filename: string;
      startLine: number | null;
      endLine: number | null;
      totalLines: number | null;
      content: string;
      encoding?: string;
    }
  | { type: "list_dir"; dirname: string; entries: Array<{ name: string; path: string; type: string }>; truncated: boolean; tree?: string }
  | { type: "grep" | "rg"; pattern: string; matches: SearchMatch[]; totalMatches: number; matchedFiles: number; skippedBinary: number; truncated: boolean }
  | { type: "ask_user"; question: string }
  | { type: "memory_agent"; action: "recall" | "remember"; content: string }
  | { type: "web_search"; query: string; url: string; results: WebSearchResult[]; error: string | null }
  | { type: "generic"; name: string };

export type { ToolCallKind };

export interface UseToolCallBadgeReturn {
  kind: Ref<ToolCallBadgeProps["kind"]>;
  loading: Ref<ToolCallBadgeProps["loading"]>;
}

export function useToolCallBadge(
  props: ToolCallBadgeProps,
  _emit: ToolCallBadgeEmit,
): UseToolCallBadgeReturn {
  return {
    kind: toRef(props, "kind"),
    loading: toRef(props, "loading"),
  };
}
