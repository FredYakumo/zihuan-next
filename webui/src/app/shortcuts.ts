export interface GlobalShortcutHandlers {
  onNewGraph: () => void | Promise<void>;
  onOpenFile: () => void | Promise<void>;
  onSaveFile: () => void | Promise<void>;
  onSaveAs: () => void | Promise<void>;
}

export function registerGlobalShortcuts(handlers: GlobalShortcutHandlers): void {
  document.addEventListener("keydown", (e) => {
    const target = e.target as HTMLElement;
    const inInput = target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;
    if (!e.ctrlKey && !e.metaKey) return;

    const invoke = (action: () => void | Promise<void>) => {
      void Promise.resolve(action()).catch(console.error);
    };

    if (e.key === "n" && !e.shiftKey && !inInput) {
      e.preventDefault();
      invoke(handlers.onNewGraph);
    } else if (e.key === "o" && !e.shiftKey && !inInput) {
      e.preventDefault();
      invoke(handlers.onOpenFile);
    } else if (e.key === "s" && e.shiftKey) {
      e.preventDefault();
      invoke(handlers.onSaveAs);
    } else if (e.key === "s" && !e.shiftKey) {
      e.preventDefault();
      invoke(handlers.onSaveFile);
    }
  });
}
