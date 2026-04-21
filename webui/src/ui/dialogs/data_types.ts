/**
 * data_types.ts — Central registry for DataType strings, dropdown creation,
 * and connection-type compatibility checks.
 *
 * Keep this file in sync with the Rust `DataType` enum in
 * `crates/zihuan_node/src/data_value.rs`.
 */

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
  "RedisRef",
  "MySqlRef",
  "TavilyRef",
  "SessionStateRef",
  "OpenAIMessageSessionCacheRef",
  "LLModel",
  "LoopControlRef",
];

/**
 * Build a `<select>` element populated with all DataType options.
 *
 * If `value` is not found in DATA_TYPES (e.g. a custom or future Vec variant),
 * it is prepended as a selected option so the value is never silently lost.
 */
export function dataTypeSelect(value = "String", id?: string): HTMLSelectElement {
  const sel = document.createElement("select");
  if (id) sel.id = id;

  // Fallback: keep unknown values (e.g. Vec<CustomType>) visible
  if (value && !DATA_TYPES.includes(value)) {
    const custom = document.createElement("option");
    custom.value = value;
    custom.textContent = value;
    custom.selected = true;
    sel.appendChild(custom);
  }

  for (const dt of DATA_TYPES) {
    const opt = document.createElement("option");
    opt.value = dt;
    opt.textContent = dt;
    if (dt === value) opt.selected = true;
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
