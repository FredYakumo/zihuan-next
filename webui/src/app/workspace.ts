export const WORKSPACE_KEY = "zh-workspace";

export function clearWorkspaceState(): void {
  localStorage.removeItem(WORKSPACE_KEY);
}

export function tabNameFrom(filePath: string | null, fallback = "未命名"): string {
  if (!filePath) return fallback;
  const base = filePath.split(/[\\/]/).pop() ?? fallback;
  return base.replace(/\.json$/i, "");
}
