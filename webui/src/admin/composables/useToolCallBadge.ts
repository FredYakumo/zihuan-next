export interface ToolCallBadgeProps {
  kind: ToolCallKind;
  loading?: boolean;
}

export type ToolCallBadgeEmit = (e: "click") => void;

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
  | {
      type: "edit_file";
      filename: string;
      addedLines: number;
      removedLines: number;
      edits: LineEditSpec[];
    }
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

export type { ToolCallKind, LineEditSpec };

export interface UseToolCallBadgeReturn {
  kind: ToolCallBadgeProps["kind"];
  loading: ToolCallBadgeProps["loading"];
}

export function useToolCallBadge(
  props: ToolCallBadgeProps,
  _emit: ToolCallBadgeEmit,
): UseToolCallBadgeReturn {
  return {
    kind: props.kind,
    loading: props.loading,
  };
}
