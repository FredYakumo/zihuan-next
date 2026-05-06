/**
 * data_types.ts — Central registry for DataType strings, dropdown creation,
 * and connection-type compatibility checks.
 *
 * Keep this file in sync with the Rust `DataType` enum in
 * `zihuan_graph_engine/src/data_value.rs`.
 */

import type { DataTypeMetaData } from "../../api/types";

/** Deep-clone a DataTypeMetaData value so editor state can mutate safely. */
export function cloneDataTypeMetaData(dt: DataTypeMetaData): DataTypeMetaData {
  if (typeof dt === "string") return dt;
  if (dt !== null && typeof dt === "object" && !Array.isArray(dt)) {
    if ("Vec" in dt) {
      return { Vec: cloneDataTypeMetaData(dt.Vec) };
    }
    if ("Custom" in dt) {
      return { Custom: dt.Custom };
    }
  }
  return "Any";
}

/** Parse a display string such as `Vec<OpenAIMessage>` back into raw DataType metadata. */
export function parseDisplayDataType(value: string): DataTypeMetaData {
  const trimmed = value.trim();
  if (!trimmed) return "Any";
  if (trimmed.startsWith("Vec<") && trimmed.endsWith(">")) {
    return { Vec: parseDisplayDataType(trimmed.slice(4, -1)) };
  }
  if (trimmed.startsWith("Custom<") && trimmed.endsWith(">")) {
    return { Custom: trimmed.slice(7, -1).trim() };
  }
  if (trimmed.startsWith("Custom(") && trimmed.endsWith(")")) {
    return { Custom: trimmed.slice(7, -1).trim() };
  }
  return trimmed;
}

/** All selectable DataType strings, ordered by category. */
export const DATA_TYPES: readonly string[] = [
  // Primitives
  "String",
  "Integer",
  "Float",
  "Boolean",
  "Json",
  "Any",
  "Binary",
  "Password",

  // Message / Bot types
  "MessageEvent",
  "OpenAIMessage",
  "QQMessage",
  "FunctionTools",

  // Vec variants
  "Vec<String>",
  "Vec<Integer>",
  "Vec<Float>",
  "Vec<Boolean>",
  "Vec<Json>",
  "Vec<Any>",
  "Vec<OpenAIMessage>",
  "Vec<QQMessage>",

  // Reference handles
  "BotAdapterRef",
  "S3Ref",
  "RedisRef",
  "MySqlRef",
  "TavilyRef",
  "SessionStateRef",
  "OpenAIMessageSessionCacheRef",
  "LLModel",
  "LoopControlRef",
];

/**
 * Normalize a DataType value from the backend (either a plain string like "String"
 * or a Rust serde JSON object like {"Vec": "OpenAIMessage"}) into a display string.
 *
 * Handles nested Vec types recursively, e.g. {"Vec": {"Vec": "String"}} → "Vec<Vec<String>>".
 */
export function normalizeDataType(dt: DataTypeMetaData): string {
  if (typeof dt === "string") return dt;
  if (dt !== null && typeof dt === "object" && !Array.isArray(dt)) {
    const keys = Object.keys(dt as object);
    if (keys.length > 0) {
      const inner = (dt as Record<string, DataTypeMetaData>)[keys[0]];
      return `${keys[0]}<${normalizeDataType(inner)}>`;
    }
  }
  return "Any";
}

/**
 * Build a `<select>` element populated with all DataType options.
 *
 * Accepts the raw backend value (string or Rust serde JSON object) and normalises
 * it internally so the dropdown always shows a human-readable string.
 * If the normalised value is not found in DATA_TYPES (e.g. a custom or future Vec variant),
 * it is prepended as a selected option so the value is never silently lost.
 */
export function dataTypeSelect(value: DataTypeMetaData = "String", id?: string): HTMLSelectElement {
  const normalized = normalizeDataType(value);
  const sel = document.createElement("select");
  if (id) sel.id = id;

  // Fallback: keep unknown values (e.g. Vec<CustomType>) visible
  if (normalized && !DATA_TYPES.includes(normalized)) {
    const custom = document.createElement("option");
    custom.value = normalized;
    custom.textContent = normalized;
    custom.selected = true;
    sel.appendChild(custom);
  }

  for (const dt of DATA_TYPES) {
    const opt = document.createElement("option");
    opt.value = dt;
    opt.textContent = dt;
    if (dt === normalized) opt.selected = true;
    sel.appendChild(opt);
  }
  return sel;
}

/** Check whether two DataType strings are wire-compatible. */
export function isValidConnectionType(a: string, b: string): boolean {
  if (!a || !b) return true;
  if (a === "*" || b === "*") return true;
  const lower = (s: string) => s.toLowerCase();
  if (lower(a) === "any" || lower(b) === "any") return true;
  const isVecAny = (t: string) => /^Vec<Any>$/i.test(t);
  const isVec = (t: string) => /^Vec<.+>/i.test(t);
  if ((isVecAny(a) && isVec(b)) || (isVecAny(b) && isVec(a))) return true;
  return a === b;
}
