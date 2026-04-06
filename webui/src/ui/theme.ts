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
  linkColor: string;
  eventLinkColor: string;
  connectingLinkColor: string;
}

const DARK_LITEGRAPH: LiteGraphColors = {
  canvasBg:           "#120d1e",
  gridDotColor:       "#1e1535",
  nodeBg:             "#1e1332",
  nodeHeader:         "#2a1848",
  nodeTitleText:      "#ede0f5",
  nodeSelectedTitle:  "#ffffff",
  nodeText:           "#b8a0cc",
  nodeBox:            "#7a50b0",
  nodeBoxOutline:     "#9a70d0",
  shadow:             "rgba(0,0,0,0.5)",
  widgetBg:           "#0f0a1e",
  widgetOutline:      "#2e1a4a",
  widgetText:         "#ede0f5",
  widgetSecondary:    "#8a70a0",
  widgetDisabled:     "#4a3860",
  linkColor:          "#8060b0",
  eventLinkColor:     "#d07bb8",
  connectingLinkColor:"#c088d8",
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
  linkColor:          "#9060c0",
  eventLinkColor:     "#a03090",
  connectingLinkColor:"#7030a8",
};

/** Return the LiteGraph color set matching the currently active theme. */
export function getLiteGraphColors(): LiteGraphColors {
  const tag = document.documentElement.dataset.theme;
  const isDark = tag ? tag === "dark" : !window.matchMedia("(prefers-color-scheme: light)").matches;
  return isDark ? DARK_LITEGRAPH : LIGHT_LITEGRAPH;
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
