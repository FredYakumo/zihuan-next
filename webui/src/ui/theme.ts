// Zihuan Next — Theme detection and switching

export type Theme = "dark" | "light";

const STORAGE_KEY = "zh-theme";

// ─── Theme-change listeners ───────────────────────────────────────────────────

const themeListeners: Array<() => void> = [];

/** Register a callback invoked whenever the active theme changes. Returns an unsubscribe fn. */
export function onThemeChange(callback: () => void): () => void {
  themeListeners.push(callback);
  return () => {
    const i = themeListeners.indexOf(callback);
    if (i >= 0) themeListeners.splice(i, 1);
  };
}

function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  for (const cb of themeListeners) cb();
}

// ─── LiteGraph color tokens ───────────────────────────────────────────────────

/** Per-DataType colour categories for link wires and port dots. */
export interface LinkTypeColors {
  primitive: string;   // String, Integer, Float, Boolean, Binary, Password
  complex:   string;   // Json, MessageEvent, OpenAIMessage, QQMessage, FunctionTools, LLModel
  ref:       string;   // *Ref types, LoopControlRef
  array:     string;   // Vec(...) types
  any:       string;   // Any / wildcard
}

export interface LiteGraphColors {
  canvasBg: string;
  gridDotColor: string;
  nodeBg: string;
  nodeHeader: string;
  nodeTitleText: string;
  nodeSelectedTitle: string;
  nodeText: string;
  nodeBox: string;
  nodeBoxOutline: string;
  shadow: string;
  widgetBg: string;
  widgetOutline: string;
  widgetText: string;
  widgetSecondary: string;
  widgetDisabled: string;
  widgetButtonBg: string;    // button widget background (overrides LiteGraph's hardcoded #222)
  widgetButtonText: string;  // button widget label text
  linkColor: string;
  eventLinkColor: string;
  connectingLinkColor: string;
  linkTypeColors: LinkTypeColors;
  boundaryNodeHeader: string;  // function_inputs / function_outputs header color
  boundaryNodeBg: string;      // function_inputs / function_outputs background color
}

const DARK_LITEGRAPH: LiteGraphColors = {
  canvasBg:           "#111114",
  gridDotColor:       "#1c1c22",
  nodeBg:             "#181820",
  nodeHeader:         "#2a1848",
  nodeTitleText:      "#e8e0f0",
  nodeSelectedTitle:  "#ffffff",
  nodeText:           "#a090b8",
  nodeBox:            "#7a50b0",
  nodeBoxOutline:     "#9a70d0",
  shadow:             "rgba(0,0,0,0.6)",
  widgetBg:           "#0e0e12",
  widgetOutline:      "#28283a",
  widgetText:         "#e8e0f0",
  widgetSecondary:    "#706880",
  widgetDisabled:     "#383048",
  widgetButtonBg:     "#2a1848",
  widgetButtonText:   "#e8e0f0",
  linkColor:          "#aaaaaa",
  eventLinkColor:     "#cccccc",
  connectingLinkColor:"#ffffff",
  boundaryNodeHeader: "#0d4d48",  // deep teal
  boundaryNodeBg:     "#051f1c",  // very dark teal
  linkTypeColors: {
    primitive: "#7ec8ff",   // sky blue  — String/Int/Float/Bool/Binary/Password
    complex:   "#ffb347",   // amber     — Json/MessageEvent/OpenAIMessage/QQMessage/FunctionTools/LLModel
    ref:       "#4dd9a0",   // teal      — *Ref types / LoopControlRef
    array:     "#c89bff",   // lavender  — Vec(...)
    any:       "#aaaaaa",   // gray      — Any / wildcard
  },
};

const LIGHT_LITEGRAPH: LiteGraphColors = {
  canvasBg:           "#fdf8ff",
  gridDotColor:       "#e0cef5",
  nodeBg:             "#f0e8fd",
  nodeHeader:         "#ddc8f5",
  nodeTitleText:      "#1e0f35",
  nodeSelectedTitle:  "#0d0020",
  nodeText:           "#6a4890",
  nodeBox:            "#8040b0",
  nodeBoxOutline:     "#9050c0",
  shadow:             "rgba(100,50,150,0.12)",
  widgetBg:           "#f8f2fd",
  widgetOutline:      "#d8c0ec",
  widgetText:         "#1e0f35",
  widgetSecondary:    "#6a4890",
  widgetDisabled:     "#9a78b8",
  widgetButtonBg:     "#ddc8f5",
  widgetButtonText:   "#1e0f35",
  linkColor:          "#9060c0",
  eventLinkColor:     "#a03090",
  connectingLinkColor:"#7030a8",
  boundaryNodeHeader: "#0d9d92",  // bright teal
  boundaryNodeBg:     "#c5eee9",  // light teal
  linkTypeColors: {
    primitive: "#3a80c0",   // blue
    complex:   "#cc7000",   // dark amber
    ref:       "#1d8a60",   // dark teal
    array:     "#7040b0",   // purple
    any:       "#606070",   // gray
  },
};

/**
 * Return the LiteGraph color set matching the currently active theme.
 */
export function getLiteGraphColors(): LiteGraphColors {
  const tag = document.documentElement.dataset.theme;
  const isDark = tag ? tag === "dark" : !window.matchMedia("(prefers-color-scheme: light)").matches;
  return isDark ? DARK_LITEGRAPH : LIGHT_LITEGRAPH;
}

/**
 * Return colors for function_inputs and function_outputs boundary nodes.
 * These colors adapt to the current theme.
 */
export function getBoundaryNodeColors(): { header: string; bg: string } {
  const colors = getLiteGraphColors();
  return {
    header: colors.boundaryNodeHeader,
    bg: colors.boundaryNodeBg,
  };
}

function getSystemTheme(): Theme {
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

/**
 * Read stored preference or fall back to system colour scheme.
 * Applies the resolved theme and starts watching for system changes.
 * Call this before any DOM is built to avoid a flash of wrong theme.
 */
export function initTheme(): void {
  const stored = localStorage.getItem(STORAGE_KEY) as Theme | null;
  applyTheme(stored ?? getSystemTheme());

  // Live-update when the OS switches, but only if the user hasn't pinned a theme.
  window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", (e) => {
    if (!localStorage.getItem(STORAGE_KEY)) {
      applyTheme(e.matches ? "light" : "dark");
    }
  });
}

/** Explicitly set and persist a theme choice. */
export function setTheme(theme: Theme): void {
  localStorage.setItem(STORAGE_KEY, theme);
  applyTheme(theme);
}

/** Clear stored preference and revert to the current system theme. */
export function clearTheme(): void {
  localStorage.removeItem(STORAGE_KEY);
  applyTheme(getSystemTheme());
}

// ─── Per-type port/link colour helpers ───────────────────────────────────────

const COMPLEX_TYPES = new Set([
  "Json", "MessageEvent", "OpenAIMessage", "QQMessage", "FunctionTools", "LLModel",
]);

/**
 * Return the theme-aware colour for a given DataType string.
 * Matches the same categorisation used for link_type_colors.
 */
export function getPortColor(typeStr: string): string {
  const tc = getLiteGraphColors().linkTypeColors;
  if (!typeStr || typeStr === "Any" || typeStr === "*") return tc.any;
  if (typeStr.startsWith("Vec")) return tc.array;
  if (typeStr.endsWith("Ref") || typeStr === "LoopControlRef") return tc.ref;
  if (COMPLEX_TYPES.has(typeStr)) return tc.complex;
  return tc.primitive;
}
