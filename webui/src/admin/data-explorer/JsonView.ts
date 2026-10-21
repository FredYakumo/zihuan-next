import { computed, ref, watch } from "vue";

export type JsonScalarKind = "string" | "number" | "boolean" | "null";
export type JsonContainerKind = "object" | "array";
export type JsonKind = JsonScalarKind | JsonContainerKind;

/** One rendered line of the JSON tree. Closing brace rows carry `close`. */
export interface JsonRow {
  id: string;
  depth: number;
  /** Object key or array index; absent on the root and closing rows. */
  key?: string;
  isIndex: boolean;
  kind: JsonKind;
  /** Container row (object/array). */
  container: boolean;
  /** Container with no children, rendered inline as `{}` / `[]`. */
  empty: boolean;
  /** Container children or a long string are currently shown in full. */
  expanded: boolean;
  /** Row can be toggled: a non-empty container or a long string. */
  expandable: boolean;
  /** Number of children for containers. */
  entries: number;
  /** Scalar display text, already quoted/escaped. */
  text?: string;
  /** Scalar text shown when a long string is collapsed. */
  preview?: string;
  close: boolean;
  comma: boolean;
}

interface NormalizedJson {
  data: unknown;
  /** Text used by "复制原始数据" and the raw view. */
  raw: string;
  /** False when the source is a plain string that is not JSON. */
  json: boolean;
}

/** Containers up to this depth start expanded. */
const AUTO_EXPAND_DEPTH = 2;
/** Containers larger than this start collapsed so huge results stay cheap to render. */
const AUTO_EXPAND_MAX_ENTRIES = 50;
/** Strings longer than this collapse to a preview until toggled. */
const LONG_STRING_LIMIT = 120;

function kindOf(value: unknown): JsonKind {
  if (value === null || value === undefined) return "null";
  if (Array.isArray(value)) return "array";
  switch (typeof value) {
    case "object":
      return "object";
    case "number":
      return "number";
    case "boolean":
      return "boolean";
    default:
      return "string";
  }
}

function entriesOf(value: unknown, kind: JsonKind): [string, unknown][] {
  if (kind === "array") {
    return (value as unknown[]).map((item, index) => [String(index), item]);
  }
  if (kind === "object") {
    return Object.entries(value as Record<string, unknown>);
  }
  return [];
}

/** Stable row id for a path; JSON-encoding keeps keys with dots or brackets distinct. */
function pathId(path: string[]): string {
  return JSON.stringify(path);
}

function normalizeSource(source: unknown): NormalizedJson {
  if (typeof source === "string") {
    const trimmed = source.trim();
    if (trimmed) {
      try {
        const data = JSON.parse(trimmed);
        if (data !== null && typeof data === "object") {
          return { data, raw: trimmed, json: true };
        }
      } catch {
        // Not JSON; fall through and render the text as-is.
      }
    }
    return { data: source, raw: source, json: false };
  }
  if (source === undefined) {
    return { data: null, raw: "null", json: true };
  }
  return {
    data: source,
    raw: JSON.stringify(source, null, 2) ?? String(source),
    json: true,
  };
}

function defaultExpanded(data: unknown): Set<string> {
  const expanded = new Set<string>();
  collect(data, [], 0);
  return expanded;

  function collect(value: unknown, path: string[], depth: number) {
    const kind = kindOf(value);
    if (kind !== "object" && kind !== "array") return;
    const children = entriesOf(value, kind);
    if (!children.length || depth >= AUTO_EXPAND_DEPTH) return;
    if (children.length > AUTO_EXPAND_MAX_ENTRIES) return;
    expanded.add(pathId(path));
    for (const [childKey, childValue] of children) {
      collect(childValue, [...path, childKey], depth + 1);
    }
  }
}

function buildRows(data: unknown, expanded: Set<string>): JsonRow[] {
  const rows: JsonRow[] = [];
  appendRows(rows, data, undefined, false, 0, [], true, expanded);
  return rows;
}

function appendRows(
  rows: JsonRow[],
  value: unknown,
  key: string | undefined,
  isIndex: boolean,
  depth: number,
  path: string[],
  isLast: boolean,
  expanded: Set<string>,
) {
  const kind = kindOf(value);
  const id = pathId(path);
  const comma = !isLast;

  if (kind === "object" || kind === "array") {
    const children = entriesOf(value, kind);
    const isExpanded = children.length > 0 && expanded.has(id);
    rows.push({
      id,
      depth,
      key,
      isIndex,
      kind,
      container: true,
      empty: children.length === 0,
      expanded: isExpanded,
      expandable: children.length > 0,
      entries: children.length,
      close: false,
      comma: isExpanded ? false : comma,
    });
    if (isExpanded) {
      children.forEach(([childKey, childValue], index) => {
        appendRows(
          rows,
          childValue,
          childKey,
          kind === "array",
          depth + 1,
          [...path, childKey],
          index === children.length - 1,
          expanded,
        );
      });
      rows.push({
        id: `${id}#close`,
        depth,
        isIndex: false,
        kind,
        container: true,
        empty: false,
        expanded: false,
        expandable: false,
        entries: 0,
        close: true,
        comma,
      });
    }
    return;
  }

  const rawString = kind === "string" ? String(value) : undefined;
  const scalar = kind === "null" ? null : value;
  const text = rawString !== undefined ? JSON.stringify(rawString) : String(scalar);
  const expandable = rawString !== undefined && text.length > LONG_STRING_LIMIT;
  const textId = `${id}#text`;
  const textExpanded = expandable && expanded.has(textId);
  rows.push({
    id: textId,
    depth,
    key,
    isIndex,
    kind,
    container: false,
    empty: false,
    expanded: textExpanded,
    expandable,
    entries: 0,
    text,
    // Slice the raw string, not the escaped text, so a boundary never splits an escape.
    preview: expandable
      ? JSON.stringify(`${Array.from(rawString).slice(0, LONG_STRING_LIMIT).join("")}…`)
      : undefined,
    close: false,
    comma,
  });
}

export function useJsonView(source: () => unknown) {
  const normalized = computed(() => normalizeSource(source()));
  const expanded = ref<Set<string>>(new Set());
  const mode = ref<"tree" | "raw">("tree");
  const collapsed = ref(false);
  const copied = ref(false);

  watch(
    () => normalized.value.data,
    (data) => {
      // Only the expansion defaults follow the data; view mode and the user's own collapse
      // choice survive live updates such as a tool result arriving.
      expanded.value = defaultExpanded(data);
    },
    { immediate: true },
  );

  /** The tree is only worth offering when the data really is structured JSON. */
  const canToggle = computed(
    () => normalized.value.json && typeof normalized.value.data === "object" && normalized.value.data !== null,
  );

  const canCopy = computed(() => normalized.value.raw.length > 0);

  const rows = computed(() => buildRows(normalized.value.data, expanded.value));

  const toggleTitle = computed(() =>
    mode.value === "tree" ? "切换到原始 JSON" : "切换到优化视图",
  );

  function toggle(id: string) {
    const next = new Set(expanded.value);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    expanded.value = next;
  }

  function toggleMode() {
    mode.value = mode.value === "tree" ? "raw" : "tree";
  }

  async function copyRaw() {
    try {
      await navigator.clipboard.writeText(normalized.value.raw);
      copied.value = true;
      window.setTimeout(() => {
        copied.value = false;
      }, 1600);
    } catch (error) {
      alert(`复制失败：${error instanceof Error ? error.message : String(error)}`);
    }
  }

  return {
    normalized,
    rows,
    mode,
    collapsed,
    copied,
    canToggle,
    canCopy,
    toggleTitle,
    toggle,
    toggleMode,
    copyRaw,
  };
}
