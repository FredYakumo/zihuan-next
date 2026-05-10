import { fileIO, graphs, tasks, workflows as workflowsApi } from "../api/client";
import type { NodeTypeInfo } from "../api/types";
import type { ZihuanCanvas } from "../graph/canvas";
import {
  openGraphIODialog,
  openGraphMetadataDialog,
  openHyperparametersDialog,
  openVariablesDialog,
  showAddNodeDialog,
  showErrorDialog,
  showWorkflowBrowserDialog,
  showWorkflowsDialog,
} from "../ui/dialogs/index";
import type { TabManager } from "./tab_manager";
import { tabNameFrom } from "./workspace";

export interface GraphActionsOptions {
  canvas: ZihuanCanvas;
  tabs: TabManager;
  getNodeTypes: () => NodeTypeInfo[];
  getCurrentTaskId: () => string | null;
  persistWorkspace: () => Promise<void>;
}

export class GraphActions {
  constructor(private readonly options: GraphActionsOptions) {}

  private async openWorkflowSetTab(workflowPath: string): Promise<void> {
    const existing = this.options.tabs.findWorkflowTabByPath(workflowPath);
    if (existing) {
      this.options.tabs.setActiveTabId(existing.id);
      await this.options.canvas.loadExternalSession(existing.id);
      await this.options.persistWorkspace();
      return;
    }

    const openResult = await fileIO.open(workflowPath);
    const name = tabNameFrom(workflowPath);
    await this.options.tabs.openTab(openResult.session_id, name, false, true);
    this.options.tabs.updateTab(openResult.session_id, { workflowPath });
    await this.options.canvas.loadExternalSession(openResult.session_id);
    await this.options.persistWorkspace();
  }

  private summarizeErrorMessage(error: unknown, fallback: string): string {
    const message = error instanceof Error ? error.message : String(error ?? "");
    return message
      .split("\n")
      .map((line) => line.trim())
      .find((line) => line.length > 0)
      ?? fallback;
  }

  async openLocalFile(): Promise<void> {
    if ("showOpenFilePicker" in window) {
      type ShowOpenFilePicker = (opts?: object) => Promise<FileSystemFileHandle[]>;
      try {
        const [handle] = await (window.showOpenFilePicker as ShowOpenFilePicker)({
          types: [{ description: "JSON", accept: { "application/json": [".json"] } }],
          multiple: false,
        });
        const file = await handle.getFile();
        const result = await fileIO.upload(file);
        const name = file.name.replace(/\.json$/i, "");
        await this.options.tabs.openTab(result.session_id, name, false, false);
        this.options.tabs.updateTab(result.session_id, { fileHandle: handle });
        await this.options.canvas.loadExternalSession(result.session_id);
        await this.options.persistWorkspace();
        return;
      } catch (e) {
        if ((e as Error).name === "AbortError") return;
      }
    }

    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        const result = await fileIO.upload(file);
        const name = file.name.replace(/\.json$/i, "");
        await this.options.tabs.openTab(result.session_id, name, false, false);
        await this.options.canvas.loadExternalSession(result.session_id);
        await this.options.persistWorkspace();
      } catch (e) {
        showErrorDialog(`打开文件失败: ${(e as Error).message}`);
      }
    };
    input.click();
  }

  async openWorkflowSetPicker(): Promise<void> {
    try {
      const result = await workflowsApi.list();
      if (result.files.length === 0) {
        alert("workflow_set/ 目录中没有节点图文件。");
        return;
      }
      const selected = await showWorkflowsDialog(result.files);
      if (!selected) return;
      await this.openWorkflowSetTab("workflow_set/" + selected);
    } catch (e) {
      showErrorDialog(`打开 workflow 失败: ${(e as Error).message}`);
    }
  }

  async browseWorkflows(): Promise<void> {
    try {
      const result = await workflowsApi.listDetailed();
      const selected = await showWorkflowBrowserDialog(result.workflows);
      if (!selected) return;
      await this.openWorkflowSetTab("workflow_set/" + selected);
    } catch (e) {
      showErrorDialog(`打开 workflow 失败: ${(e as Error).message}`);
    }
  }

  async validate(): Promise<void> {
    const sid = this.options.canvas.isInSubgraph
      ? this.options.canvas.rootSessionId
      : this.options.canvas.sessionId;
    if (!sid) {
      showErrorDialog("请先打开一个节点图");
      return;
    }
    try {
      await this.options.canvas.syncInlineWidgetValues();
      await this.options.canvas.flushPendingWidgetMutations();
      if (this.options.canvas.isInSubgraph) {
        await this.options.canvas.flushSubgraphToRoot();
      }
      const result = await graphs.validate(sid);
      if (result.has_errors) {
        const msgs = result.issues.map((issue) => `[${issue.severity}] ${issue.message}`).join("\n");
        showErrorDialog(`验证失败:\n\n${msgs}`);
      }
    } catch (e) {
      showErrorDialog(`验证失败: ${(e as Error).message}`);
    }
  }

  async execute(): Promise<void> {
    const sid = this.options.canvas.isInSubgraph
      ? this.options.canvas.rootSessionId
      : this.options.canvas.sessionId;
    if (!sid) {
      showErrorDialog("请先打开一个节点图");
      return;
    }
    try {
      await this.options.canvas.syncInlineWidgetValues();
      await this.options.canvas.flushPendingWidgetMutations();
      if (this.options.canvas.isInSubgraph) {
        await this.options.canvas.flushSubgraphToRoot();
      }
      await graphs.execute(sid);
    } catch (e) {
      const summary = this.summarizeErrorMessage(e, "执行失败");
      showErrorDialog(`执行失败: ${summary}\n\n详细信息请到任务管理器的“日志”中查看。`);
    }
  }

  async stopTask(): Promise<void> {
    const taskId = this.options.getCurrentTaskId();
    if (!taskId) {
      showErrorDialog("当前没有正在运行的任务");
      return;
    }
    try {
      await tasks.stop(taskId);
    } catch (e) {
      showErrorDialog(`停止任务失败: ${(e as Error).message}`);
    }
  }

  async openHyperparameters(): Promise<void> {
    const sid = this.options.canvas.rootSessionId;
    if (!sid) {
      showErrorDialog("请先打开一个节点图");
      return;
    }
    const openDialog = () =>
      openHyperparametersDialog(sid, () => {
        this.options.canvas.reloadCurrentSession().catch(console.error);
      }).catch(console.error);

    if (this.options.canvas.isInSubgraph) {
      this.options.canvas.syncInlineWidgetValues().then(() =>
        this.options.canvas.flushPendingWidgetMutations()
      ).then(() =>
        this.options.canvas.flushSubgraphToRoot()
      ).then(openDialog).catch((e: Error) => {
        showErrorDialog(`同步子图失败: ${e.message}`);
      });
      return;
    }

    await this.options.canvas.syncInlineWidgetValues();
    await this.options.canvas.flushPendingWidgetMutations();
    openDialog();
  }

  async openVariables(): Promise<void> {
    const sid = this.options.canvas.sessionId;
    if (!sid) {
      showErrorDialog("请先打开一个节点图");
      return;
    }
    await this.options.canvas.syncInlineWidgetValues();
    await this.options.canvas.flushPendingWidgetMutations();
    openVariablesDialog(sid, () => {
      this.options.canvas.reloadCurrentSession().catch(console.error);
    }).catch(console.error);
  }

  async addNodeWithDialog(graphX?: number, graphY?: number): Promise<void> {
    const sid = this.options.canvas.sessionId;
    if (!sid) {
      alert("请先打开一个节点图");
      return;
    }
    const typeId = await showAddNodeDialog(this.options.getNodeTypes());
    if (!typeId) return;
    const pos = graphX !== undefined && graphY !== undefined
      ? { x: graphX, y: graphY }
      : this.options.canvas.graphCenterPos();
    try {
      await graphs.addNode(sid, typeId, undefined, pos.x, pos.y);
      await this.options.canvas.reloadCurrentSession();
      this.options.tabs.setTabDirty(this.options.canvas.rootSessionId ?? sid, true);
    } catch (e) {
      console.error("addNode error:", e);
      showErrorDialog(`添加节点失败: ${(e as Error).message}`);
    }
  }

  async openGraphMetadata(): Promise<void> {
    const activeTabId = this.options.tabs.getActiveTabId();
    if (!activeTabId) {
      showErrorDialog("请先打开或新建一个节点图。");
      return;
    }
    await openGraphMetadataDialog(activeTabId, () => {
      this.options.tabs.setTabDirty(activeTabId, true);
    });
  }

  async openGraphIO(): Promise<void> {
    const activeTabId = this.options.tabs.getActiveTabId();
    if (!activeTabId) {
      showErrorDialog("请先打开或新建一个节点图。");
      return;
    }
    await this.options.canvas.syncInlineWidgetValues();
    await this.options.canvas.flushPendingWidgetMutations();
    if (this.options.canvas.isInSubgraph) {
      await this.options.canvas.flushSubgraphToRoot();
    }
    await openGraphIODialog(activeTabId, () => {
      this.options.tabs.setTabDirty(activeTabId, true);
      this.options.canvas.reloadCurrentSession().catch(console.error);
    });
  }
}
