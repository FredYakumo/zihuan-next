// Zihuan Next — Dynamic theme system

export interface LinkTypeColors {
  primitive: string;
  complex: string;
  ref: string;
  array: string;
  any: string;
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
  widgetButtonBg: string;
  widgetButtonText: string;
  linkColor: string;
  eventLinkColor: string;
  connectingLinkColor: string;
  linkHalo: string;
  highlightedLinkColor: string;
  linkLabelBg: string;
  linkLabelText: string;
  linkTypeColors: LinkTypeColors;
  boundaryNodeHeader: string;
  boundaryNodeBg: string;
}

export interface ThemeConfig {
  name: string;
  display_name: string;
  mode: string;
  css: Record<string, string>;
  litegraph: LiteGraphColors;
}

const STORAGE_KEY = "zh-theme";

const themeListeners: Array<() => void> = [];

let allThemes: Map<string, ThemeConfig> = new Map();
let currentThemeName = "default_dark";

// ─── Built-in default themes ──────────────────────────────────────────────────

const DEFAULT_DARK: ThemeConfig = {
  name: "default_dark",
  display_name: "默认暗色",
  mode: "dark",
  css: {
    "--bg": "#050505",
    "--bg-deep": "#0f0f0f",
    "--toolbar-bg": "#1a1a1a",
    "--text": "#e6e6e6",
    "--text-muted": "#a0a0a0",
    "--text-dim": "#707070",
    "--text-faint": "#505050",
    "--text-faint2": "#383838",
    "--accent": "#3b82f6",
    "--accent-subtle": "rgba(59, 130, 246, 0.12)",
    "--border": "#222222",
    "--node-hover": "#1c1c1c",
    "--tab-inactive": "#252525",
    "--link": "#60a5fa",
    "--run-color": "#22c55e",
    "--input-bg": "#0a0a0a",
    "--btn-bg": "#1e1e1e",
    "--btn-hover": "#2a2a2a",
    "--btn-primary": "#2563eb",
    "--btn-primary-hover": "#3b82f6",
    "--btn-primary-text": "#ffffff",
    "--tool-card-bg": "#0a0a0a",
    "--tool-card-summary": "#60a5fa",
    "--float-bg": "rgba(13, 13, 13, 0.95)",
    "--toast-text": "#e6e6e6",
    "--log-stream-bg": "#0a0a0a",
    "--badge-info-bg": "#0d2d1a",
    "--badge-info-text": "#4ade80",
    "--badge-warn-bg": "#3a2a00",
    "--badge-warn-text": "#facc15",
    "--badge-error-bg": "#3a0000",
    "--badge-error-text": "#f87171",
    "--badge-debug-bg": "#0a1a3a",
    "--badge-debug-text": "#60a5fa",
    "--badge-trace-bg": "#1a1a1a",
    "--badge-trace-text": "#a0a0a0",
  },
  litegraph: {
    canvasBg: "#050505",
    gridDotColor: "#111111",
    nodeBg: "#252525",
    nodeHeader: "#353535",
    nodeTitleText: "#e6e6e6",
    nodeSelectedTitle: "#ffffff",
    nodeText: "#a0a0a0",
    nodeBox: "#2563eb",
    nodeBoxOutline: "#3b82f6",
    shadow: "rgba(0,0,0,0.6)",
    widgetBg: "#1c1c1c",
    widgetOutline: "#333333",
    widgetText: "#e6e6e6",
    widgetSecondary: "#707070",
    widgetDisabled: "#404040",
    widgetButtonBg: "#353535",
    widgetButtonText: "#e6e6e6",
    linkColor: "#888888",
    eventLinkColor: "#aaaaaa",
    connectingLinkColor: "#ffffff",
    linkHalo: "rgba(8, 8, 14, 0.55)",
    highlightedLinkColor: "#ffffff",
    linkLabelBg: "rgba(6, 6, 10, 0.78)",
    linkLabelText: "#ffffff",
    boundaryNodeHeader: "#2a4a45",
    boundaryNodeBg: "#0f1f1c",
    linkTypeColors: {
      primitive: "#60a5fa",
      complex: "#fbbf24",
      ref: "#4ade80",
      array: "#c084fc",
      any: "#888888",
    },
  },
};

const DEFAULT_LIGHT: ThemeConfig = {
  name: "default_light",
  display_name: "默认亮色",
  mode: "light",
  css: {
    "--bg": "#ffffff",
    "--bg-deep": "#f5f5f5",
    "--toolbar-bg": "#f0f0f0",
    "--text": "#1a1a1a",
    "--text-muted": "#525252",
    "--text-dim": "#737373",
    "--text-faint": "#a3a3a3",
    "--text-faint2": "#d4d4d4",
    "--accent": "#2563eb",
    "--accent-subtle": "rgba(37, 99, 235, 0.10)",
    "--border": "#e5e5e5",
    "--node-hover": "#f0f0f0",
    "--tab-inactive": "#e8e8e8",
    "--link": "#1d4ed8",
    "--run-color": "#16a34a",
    "--input-bg": "#fafafa",
    "--btn-bg": "#f0f0f0",
    "--btn-hover": "#e5e5e5",
    "--btn-primary": "#2563eb",
    "--btn-primary-hover": "#1d4ed8",
    "--btn-primary-text": "#ffffff",
    "--tool-card-bg": "#f8f8f8",
    "--tool-card-summary": "#1d4ed8",
    "--float-bg": "rgba(240, 240, 240, 0.95)",
    "--toast-text": "#1a1a1a",
    "--log-stream-bg": "#f5f5f5",
    "--badge-info-bg": "#dcfce7",
    "--badge-info-text": "#166534",
    "--badge-warn-bg": "#fef9c3",
    "--badge-warn-text": "#854d0e",
    "--badge-error-bg": "#fee2e2",
    "--badge-error-text": "#991b1b",
    "--badge-debug-bg": "#dbeafe",
    "--badge-debug-text": "#1e40af",
    "--badge-trace-bg": "#f5f5f5",
    "--badge-trace-text": "#525252",
  },
  litegraph: {
    canvasBg: "#ffffff",
    gridDotColor: "#e5e5e5",
    nodeBg: "#f5f5f5",
    nodeHeader: "#dbeafe",
    nodeTitleText: "#1a1a1a",
    nodeSelectedTitle: "#000000",
    nodeText: "#525252",
    nodeBox: "#2563eb",
    nodeBoxOutline: "#3b82f6",
    shadow: "rgba(0,0,0,0.08)",
    widgetBg: "#fafafa",
    widgetOutline: "#e5e5e5",
    widgetText: "#1a1a1a",
    widgetSecondary: "#737373",
    widgetDisabled: "#a3a3a3",
    widgetButtonBg: "#dbeafe",
    widgetButtonText: "#1a1a1a",
    linkColor: "#737373",
    eventLinkColor: "#525252",
    connectingLinkColor: "#2563eb",
    linkHalo: "rgba(0, 0, 0, 0.12)",
    highlightedLinkColor: "#000000",
    linkLabelBg: "rgba(30, 30, 30, 0.84)",
    linkLabelText: "#ffffff",
    boundaryNodeHeader: "#0d9d92",
    boundaryNodeBg: "#c5eee9",
    linkTypeColors: {
      primitive: "#2563eb",
      complex: "#d97706",
      ref: "#16a34a",
      array: "#7c3aed",
      any: "#737373",
    },
  },
};

// ─── Theme loading ────────────────────────────────────────────────────────────

function migrateOldThemeName(name: string | null): string | null {
  if (name === "dark") return "default_dark";
  if (name === "light") return "default_light";
  return name;
}

async function fetchWithTimeout(url: string, timeoutMs = 5000): Promise<Response> {
  const controller = new AbortController();
  const id = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(url, { signal: controller.signal });
    clearTimeout(id);
    return res;
  } catch (e) {
    clearTimeout(id);
    throw e;
  }
}

export async function loadThemes(): Promise<void> {
  allThemes.set("default_dark", DEFAULT_DARK);
  allThemes.set("default_light", DEFAULT_LIGHT);

  try {
    const res = await fetchWithTimeout("/api/themes", 3000);
    if (!res.ok) return;
    const data = await res.json();
    const items = data.themes as Array<{ name: string; display_name: string; mode: string }>;
    for (const item of items) {
      try {
        const detailRes = await fetchWithTimeout(`/api/themes/${item.name}`, 3000);
        if (detailRes.ok) {
          const config = (await detailRes.json()) as ThemeConfig;
          allThemes.set(config.name, config);
        }
      } catch {
        // ignore individual theme load failures
      }
    }
  } catch {
    // ignore — built-in themes are enough
  }

  generateThemeStyles();

  // Re-apply the stored/system theme now that its config is definitely loaded
  const stored = migrateOldThemeName(localStorage.getItem(STORAGE_KEY));
  const target = stored ?? getSystemThemeName();
  if (allThemes.has(target)) {
    currentThemeName = target;
    applyTheme(target);
  }
}

function generateThemeStyles(): void {
  let style = document.getElementById("dynamic-themes") as HTMLStyleElement | null;
  if (!style) {
    style = document.createElement("style");
    style.id = "dynamic-themes";
    document.head.appendChild(style);
  }

  let css = "";
  for (const [name, config] of allThemes) {
    css += `[data-theme="${name}"] {\n`;
    for (const [key, value] of Object.entries(config.css)) {
      css += `  ${key}: ${value};\n`;
    }
    css += "}\n";
  }
  style.textContent = css;
}

// ─── Theme query / switching ──────────────────────────────────────────────────

export function getThemeNames(): Array<{ name: string; display_name: string; mode: string }> {
  const list = Array.from(allThemes.values());
  list.sort((a, b) => a.name.localeCompare(b.name));
  return list.map((t) => ({ name: t.name, display_name: t.display_name, mode: t.mode }));
}

export function getCurrentThemeName(): string {
  return currentThemeName;
}

export function getThemeConfig(name: string): ThemeConfig | undefined {
  return allThemes.get(name);
}

export function setTheme(name: string): void {
  if (!allThemes.has(name)) return;
  currentThemeName = name;
  localStorage.setItem(STORAGE_KEY, name);
  applyTheme(name);
}

export function clearTheme(): void {
  localStorage.removeItem(STORAGE_KEY);
  const sys = getSystemThemeName();
  currentThemeName = sys;
  applyTheme(sys);
}

function applyTheme(name: string): void {
  document.documentElement.dataset.theme = name;
  for (const cb of themeListeners) cb();
}

function getSystemThemeName(): string {
  return window.matchMedia("(prefers-color-scheme: light)").matches
    ? "default_light"
    : "default_dark";
}

// ─── Init (sync, called before DOM is built) ──────────────────────────────────

/**
 * Call this before any DOM is built to avoid a flash of wrong theme.
 * Only sets the dataset attribute; actual styles are injected later by loadThemes().
 */
export function initTheme(): void {
  const stored = migrateOldThemeName(localStorage.getItem(STORAGE_KEY));
  const name = stored ?? getSystemThemeName();
  currentThemeName = name;
  document.documentElement.dataset.theme = name;

  // Watch system changes when no user preference is stored
  window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", (e) => {
    if (!localStorage.getItem(STORAGE_KEY)) {
      const sys = e.matches ? "default_light" : "default_dark";
      currentThemeName = sys;
      applyTheme(sys);
    }
  });
}

// ─── Theme-change listeners ───────────────────────────────────────────────────

export function onThemeChange(callback: () => void): () => void {
  themeListeners.push(callback);
  return () => {
    const i = themeListeners.indexOf(callback);
    if (i >= 0) themeListeners.splice(i, 1);
  };
}

// ─── LiteGraph color helpers ──────────────────────────────────────────────────

export function getLiteGraphColors(): LiteGraphColors {
  return allThemes.get(currentThemeName)?.litegraph ?? DEFAULT_DARK.litegraph;
}

export function getBoundaryNodeColors(): { header: string; bg: string } {
  const colors = getLiteGraphColors();
  return {
    header: colors.boundaryNodeHeader,
    bg: colors.boundaryNodeBg,
  };
}

// ─── Per-type port/link colour helpers ────────────────────────────────────────

const COMPLEX_TYPES = new Set([
  "Json", "MessageEvent", "OpenAIMessage", "QQMessage", "FunctionTools", "LLModel",
]);

export function getPortColor(typeStr: string): string {
  const tc = getLiteGraphColors().linkTypeColors;
  if (!typeStr || typeStr === "Any" || typeStr === "*") return tc.any;
  if (typeStr.startsWith("Vec")) return tc.array;
  if (typeStr.endsWith("Ref") || typeStr === "LoopControlRef") return tc.ref;
  if (COMPLEX_TYPES.has(typeStr)) return tc.complex;
  return tc.primitive;
}
