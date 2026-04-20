export function startAutoSaveLoop(
  autoSave: () => Promise<void>,
  intervalMs = 30_000,
): number {
  return window.setInterval(() => {
    autoSave().catch(console.error);
  }, intervalMs);
}

export function registerUnsavedChangesWarning(hasDirtyTabs: () => boolean): void {
  window.addEventListener("beforeunload", (e) => {
    if (hasDirtyTabs()) {
      e.preventDefault();
    }
  });
}

export function observeCanvasResize(
  canvasContainer: HTMLElement,
  onResize: () => void,
): ResizeObserver {
  onResize();
  const observer = new ResizeObserver(onResize);
  observer.observe(canvasContainer);
  return observer;
}
