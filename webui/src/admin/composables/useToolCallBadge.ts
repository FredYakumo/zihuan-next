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
