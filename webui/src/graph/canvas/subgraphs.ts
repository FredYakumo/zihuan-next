import { graphs } from "../../api/client";
import type { NodeDefinition, NodeGraphDefinition } from "../../api/types";
import { ensureToolSubgraphSignature } from "../../ui/dialogs/tool_subgraph_utils";
import type { BrainToolDefinition, EmbeddedFunctionConfig, FunctionPortDef } from "../../ui/dialogs/types";
import type { CanvasFacade } from "./types";
import { hasVisibleSubgraphContent, isNodeGraphDefinitionLike } from "./type_utils";

export class CanvasSubgraphController {
  constructor(private readonly canvas: CanvasFacade) {}

  async enterSubgraph(
    parentNodeDef: NodeDefinition,
    mode: "function" | "brain-tool",
    toolIndex?: number,
    toolDef?: BrainToolDefinition,
    functionConfig?: EmbeddedFunctionConfig,
  ): Promise<void> {
    const parentSessionId = this.canvas.state.sessionId;
    if (!parentSessionId) return;

    let subgraphDef: NodeGraphDefinition;
    let label: string;

    if (mode === "function" && functionConfig) {
      subgraphDef = functionConfig.subgraph;
      label = functionConfig.name || parentNodeDef.name;
    } else if (mode === "brain-tool" && toolDef != null) {
      const sharedInputs = (parentNodeDef.inline_values?.shared_inputs as FunctionPortDef[] | undefined) ?? [];
      subgraphDef = ensureToolSubgraphSignature(parentNodeDef.node_type, sharedInputs, toolDef).subgraph;
      label = `${parentNodeDef.name} / ${toolDef.name}`;
    } else {
      return;
    }

    const shouldRejectEmptySubgraph = !isNodeGraphDefinitionLike(subgraphDef) || (
      mode !== "brain-tool" &&
      subgraphDef.nodes.length === 0 &&
      (parentNodeDef.input_ports.length > 0 || parentNodeDef.output_ports.length > 0)
    );
    if (shouldRejectEmptySubgraph) {
      alert(
        "这个子图配置看起来已经损坏：内部节点列表为空。为避免再次覆盖已有配置，系统已阻止打开空子图。\n\n请先重新从文件加载工作流，或从历史版本恢复该函数节点。",
      );
      return;
    }

    const tab = await graphs.create();
    const virtualSessionId = tab.id;
    await graphs.put(virtualSessionId, subgraphDef);

    const saveBack = async (modifiedGraph: NodeGraphDefinition): Promise<void> => {
      if (!isNodeGraphDefinitionLike(modifiedGraph)) {
        alert("子图保存失败：收到的子图数据结构无效，已阻止覆盖父节点配置。");
        return;
      }
      const originalHadContent = hasVisibleSubgraphContent(subgraphDef);
      if (originalHadContent && modifiedGraph.nodes.length === 0) {
        alert("子图保存已阻止：修改后的子图为空，继续保存会覆盖原有内部节点。");
        return;
      }

      if (mode === "function" && functionConfig) {
        const updatedConfig: EmbeddedFunctionConfig = { ...functionConfig, subgraph: modifiedGraph };
        const parentGraph = await graphs.get(parentSessionId);
        const nodeIdx = parentGraph.nodes.findIndex((node) => node.id === parentNodeDef.id);
        if (nodeIdx >= 0) {
          parentGraph.nodes[nodeIdx] = {
            ...parentGraph.nodes[nodeIdx],
            inline_values: {
              ...parentGraph.nodes[nodeIdx].inline_values,
              function_config: updatedConfig as unknown as Record<string, unknown>,
            },
          };
          await graphs.put(parentSessionId, parentGraph);
        }
      } else if (mode === "brain-tool" && toolDef != null && toolIndex != null) {
        const parentGraph = await graphs.get(parentSessionId);
        const nodeIdx = parentGraph.nodes.findIndex((node) => node.id === parentNodeDef.id);
        if (nodeIdx >= 0) {
          const sharedInputs = (parentGraph.nodes[nodeIdx].inline_values?.shared_inputs as FunctionPortDef[] | undefined) ?? [];
          const tools: BrainToolDefinition[] = JSON.parse(
            JSON.stringify(parentGraph.nodes[nodeIdx].inline_values?.tools_config ?? []),
          );
          if (tools[toolIndex]) {
            tools[toolIndex] = ensureToolSubgraphSignature(parentGraph.nodes[nodeIdx].node_type, sharedInputs, {
              ...tools[toolIndex],
              subgraph: modifiedGraph,
            });
          }
          parentGraph.nodes[nodeIdx] = {
            ...parentGraph.nodes[nodeIdx],
            inline_values: {
              ...parentGraph.nodes[nodeIdx].inline_values,
              tools_config: tools as unknown as unknown[],
            },
          };
          await graphs.put(parentSessionId, parentGraph);
        }
      }
    };

    this.canvas.subgraphStack.push({ label, parentSessionId, virtualSessionId, saveBack });
    this.notifyNavigation();
    await this.canvas.loadSession(virtualSessionId);
  }

  async exitSubgraph(): Promise<void> {
    if (this.canvas.subgraphStack.length === 0) return;
    const entry = this.canvas.subgraphStack[this.canvas.subgraphStack.length - 1];
    if (this.canvas.state.sessionId === entry.virtualSessionId) {
      const currentGraph = await graphs.get(entry.virtualSessionId);
      await entry.saveBack(currentGraph);
    }
    try {
      await graphs.delete(entry.virtualSessionId);
    } catch {}
    this.canvas.subgraphStack.pop();
    this.notifyNavigation();
    await this.canvas.loadSession(entry.parentSessionId);
  }

  async exitSubgraphToDepth(targetDepth: number): Promise<void> {
    while (this.canvas.subgraphStack.length > targetDepth) {
      await this.exitSubgraph();
    }
  }

  async flushSubgraphToRoot(): Promise<void> {
    if (this.canvas.subgraphStack.length === 0) return;
    for (let i = this.canvas.subgraphStack.length - 1; i >= 0; i--) {
      const entry = this.canvas.subgraphStack[i];
      const graph = await graphs.get(entry.virtualSessionId);
      await entry.saveBack(graph);
    }
  }

  async loadExternalSession(sessionId: string): Promise<void> {
    if (this.canvas.subgraphStack.length > 0) {
      try {
        await this.flushSubgraphToRoot();
      } catch {}
      for (const entry of this.canvas.subgraphStack) {
        try {
          await graphs.delete(entry.virtualSessionId);
        } catch {}
      }
      this.canvas.subgraphStack = [];
      this.canvas.onNavigationChange?.([]);
    }
    await this.canvas.loadSession(sessionId);
  }

  notifyNavigation(): void {
    if (!this.canvas.onNavigationChange) return;
    this.canvas.onNavigationChange(this.canvas.subgraphStack.map((entry) => entry.label));
  }
}
