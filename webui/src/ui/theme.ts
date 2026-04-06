// Zihuan Next — Theme detection and switching

export type Theme = "dark" | "light";

const STORAGE_KEY = "zh-theme";

function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
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
