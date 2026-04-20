import type { TabInfo } from "../ui/shell/index";

export const WORKSPACE_KEY = "zh-workspace";

export interface TabSnapshot {
  workflowPath?: string;
  workflowJSON?: object;
  name: string;
  isWorkflowSet: boolean;
  canvasOffset?: [number, number];
  canvasScale?: number;
}

export interface WorkspaceState {
  tabs: TabSnapshot[];
  activeTabIndex: number;
}

export async function saveWorkspaceState(options: {
  tabList: TabInfo[];
  activeTabId: string | null;
  skipWorkspaceSave: boolean;
  getViewport: () => { offset: [number, number]; scale: number } | null;
  getGraphJson: (sessionId: string) => Promise<object>;
}): Promise<void> {
  const { tabList, activeTabId, skipWorkspaceSave, getViewport, getGraphJson } = options;
  if (skipWorkspaceSave) return;

  const viewport = getViewport();
  const tabsToSave: TabSnapshot[] = [];

  for (const tab of tabList) {
    if (tab.isWorkflowSet) {
      tabsToSave.push({
        workflowPath: `workflow_set/${tab.name}.json`,
        name: tab.name,
        isWorkflowSet: true,
        canvasOffset: tab.id === activeTabId ? viewport?.offset : undefined,
        canvasScale: tab.id === activeTabId ? viewport?.scale : undefined,
      });
    } else {
      try {
        const graphJSON = await getGraphJson(tab.id);
        tabsToSave.push({
          workflowJSON: graphJSON,
          name: tab.name,
          isWorkflowSet: false,
          canvasOffset: tab.id === activeTabId ? viewport?.offset : undefined,
          canvasScale: tab.id === activeTabId ? viewport?.scale : undefined,
        });
      } catch {
        // Skip tabs that fail to serialize.
      }
    }
  }

  const savedIndex = tabList.findIndex((tab) => tab.id === activeTabId);
  const state: WorkspaceState = {
    tabs: tabsToSave,
    activeTabIndex: savedIndex >= 0 ? savedIndex : 0,
  };

  localStorage.setItem(WORKSPACE_KEY, JSON.stringify(state));
}

export function loadWorkspaceState(): WorkspaceState | null {
  const stored = localStorage.getItem(WORKSPACE_KEY);
  if (!stored) return null;

  try {
    return JSON.parse(stored) as WorkspaceState;
  } catch {
    return null;
  }
}

export function tabNameFrom(filePath: string | null, fallback = "未命名"): string {
  if (!filePath) return fallback;
  const base = filePath.split(/[\\/]/).pop() ?? fallback;
  return base.replace(/\.json$/i, "");
}
