import { LiteGraph } from "litegraph.js";
import { graphs } from "../../api/client";
import type { EdgeDefinition, NodeDefinition, NodeGraphDefinition, NodeTypeInfo } from "../../api/types";
import { portTypeString, getNodeTypeInfo } from "../registry";
import type { ConnectPortChoice, PortSelectOption } from "../../ui/dialogs/types";
import {
  showAddNodeDialog,
  showConnectPortDialog,
  showNodeInfoDialog,
  showPortSelectDialog,
} from "../../ui/dialogs/index";
import type { CanvasFacade } from "./types";
import { findInlineInputAtPosition } from "./rendering";
import { isCompatibleTypes, visibleInputPorts } from "./type_utils";

export class CanvasInteractions {
  constructor(private readonly canvas: CanvasFacade) {}

  bind(): void {
    (LiteGraph as any).release_link_on_empty_shows_menu = true;

    const dispatchConnectionDrop = (
      sourceNode: any,
      slotFrom: any,
      isFromOutput: boolean,
      e: MouseEvent,
    ) => {
      const sourceNodeId: string | undefined = sourceNode?.zihuanId;
      if (!sourceNodeId || !slotFrom) return;
      const [gx, gy] = (this.canvas.lCanvas as any).convertEventToCanvasOffset(e) as [number, number];
      const sourceType: string = (slotFrom.type as string) ?? "*";
      this.handleConnectionDropOnEmpty(sourceNodeId, slotFrom.name as string, sourceType, isFromOutput, gx, gy).catch(console.error);
    };

    (this.canvas.lCanvas as any).showConnectionMenu = (options: any) => {
      const e: MouseEvent = options?.e;
      if (!e) return;
      if (options?.nodeFrom) dispatchConnectionDrop(options.nodeFrom, options.slotFrom, true, e);
      else if (options?.nodeTo) dispatchConnectionDrop(options.nodeTo, options.slotTo, false, e);
    };

    (this.canvas.lCanvas as any).showSearchBox = (e: MouseEvent, options: any) => {
      if (options?.node_from) dispatchConnectionDrop(options.node_from, options.slot_from, true, e);
      else if (options?.node_to) dispatchConnectionDrop(options.node_to, options.slot_to, false, e);
    };

    const canvasEl = (this.canvas.lCanvas as any).canvas as HTMLCanvasElement;
    canvasEl.addEventListener("contextmenu", (e: MouseEvent) => {
      const [gx, gy] = (this.canvas.lCanvas as any).convertEventToCanvasOffset(e) as [number, number];
      const node = this.canvas.lGraph.getNodeOnPos(gx, gy);

      if (node) {
        const found = (node as any).getSlotInPosition(gx, gy) as { slot: number; input?: unknown; output?: unknown } | null;
        if (found && found.input) {
          const portName = ((node as any).inputs?.[found.slot]?.name ?? "") as string;
          if (portName) {
            e.preventDefault();
            e.stopPropagation();
            this.showPortBindingMenu(node, found.slot, portName, e);
            return;
          }
        }

        const inlineInputHit = findInlineInputAtPosition(node, gx, gy);
        if (inlineInputHit?.input?.name) {
          e.preventDefault();
          e.stopPropagation();
          this.showPortBindingMenu(node, inlineInputHit.slot, inlineInputHit.input.name, e);
          return;
        }

        const widget = (node as any).getWidgetOnPos?.(gx, gy, true);
        if (widget) {
          const inputs = (node as any).inputs as Array<{ name: string; widget?: { name: string } }> | undefined;
          if (inputs) {
            const idx = inputs.findIndex((inp) => inp.widget && inp.widget.name === widget.name);
            if (idx >= 0) {
              e.preventDefault();
              e.stopPropagation();
              this.showPortBindingMenu(node, idx, inputs[idx].name, e);
              return;
            }
          }
        }
      }

      if (node) {
        const alreadySelected = !!(this.canvas.lCanvas as any).selected_nodes?.[node.id];
        if (!alreadySelected) (this.canvas.lCanvas as any).selectNode(node, false);
      }

      e.preventDefault();
      e.stopPropagation();
      this.showCanvasContextMenu(e, gx, gy);
    }, { capture: true });
  }

  async handleConnectionDropOnEmpty(
    sourceNodeId: string,
    sourcePortName: string,
    sourceType: string,
    isFromOutput: boolean,
    graphX: number,
    graphY: number,
  ): Promise<void> {
    const sid = this.canvas.state.sessionId;
    if (!sid) return;
    const currentNodes = this.canvas.state.graph?.nodes ?? [];
    const choice: ConnectPortChoice | null = await showConnectPortDialog(
      currentNodes,
      sourceNodeId,
      sourcePortName,
      sourceType,
      isFromOutput,
    );
    if (!choice) return;

    if (choice.kind === "existing") {
      const srcNode = this.canvas.nodeMap.get(sourceNodeId);
      const targetNode = this.canvas.nodeMap.get(choice.targetNodeId);
      if (!srcNode || !targetNode) return;
      if (isFromOutput) {
        const outSlot = (srcNode.outputs as any[]).findIndex((output: any) => output.name === sourcePortName);
        const inSlot = (targetNode.inputs as any[]).findIndex((input: any) => input.name === choice.targetPortName);
        if (outSlot >= 0 && inSlot >= 0) srcNode.connect(outSlot, targetNode, inSlot);
      } else {
        const outSlot = (targetNode.outputs as any[]).findIndex((output: any) => output.name === choice.targetPortName);
        const inSlot = (srcNode.inputs as any[]).findIndex((input: any) => input.name === sourcePortName);
        if (outSlot >= 0 && inSlot >= 0) targetNode.connect(outSlot, srcNode, inSlot);
      }
      return;
    }

    const typeId = await showAddNodeDialog(this.canvas.nodeTypes);
    if (!typeId) return;

    let newNodeId: string;
    try {
      const result = await graphs.addNode(sid, typeId, undefined, graphX, graphY);
      newNodeId = result.id;
    } catch (e) {
      console.error("[Canvas] addNode failed:", e);
      return;
    }
    await this.canvas.reloadCurrentSession();
    this.canvas.state.dirty = true;
    this.canvas.onGraphDirty?.();

    const newNodeDef = this.canvas.state.graph?.nodes.find((node) => node.id === newNodeId);
    if (!newNodeDef) return;

    const candidatePorts: PortSelectOption[] = [];
    const checkPorts = (ports: typeof newNodeDef.input_ports, wantInput: boolean) => {
      for (const port of ports) {
        if (port.hidden) continue;
        const pt = portTypeString(port.data_type);
        const compatible = isFromOutput
          ? wantInput && isCompatibleTypes(sourceType, pt)
          : !wantInput && isCompatibleTypes(sourceType, pt);
        if (compatible) candidatePorts.push({ portName: port.name, dataType: pt, isInput: wantInput });
      }
    };
    checkPorts(newNodeDef.input_ports, true);
    checkPorts(newNodeDef.output_ports, false);

    if (candidatePorts.length === 0) return;
    const chosenPort = candidatePorts.length === 1 ? candidatePorts[0] : await showPortSelectDialog(candidatePorts);
    if (!chosenPort) return;

    const srcNode = this.canvas.nodeMap.get(sourceNodeId);
    const newNode = this.canvas.nodeMap.get(newNodeId);
    if (!srcNode || !newNode) return;
    if (isFromOutput) {
      const outSlot = (srcNode.outputs as any[]).findIndex((output: any) => output.name === sourcePortName);
      const inSlot = (newNode.inputs as any[]).findIndex((input: any) => input.name === chosenPort.portName);
      if (outSlot >= 0 && inSlot >= 0) srcNode.connect(outSlot, newNode, inSlot);
    } else {
      const outSlot = (newNode.outputs as any[]).findIndex((output: any) => output.name === chosenPort.portName);
      const inSlot = (srcNode.inputs as any[]).findIndex((input: any) => input.name === sourcePortName);
      if (outSlot >= 0 && inSlot >= 0) newNode.connect(outSlot, srcNode, inSlot);
    }
  }

  showCanvasContextMenu(event: MouseEvent, graphX: number, graphY: number): void {
    document.getElementById("zh-canvas-menu")?.remove();
    const selectedNodes: any[] = Object.values((this.canvas.lCanvas as any).selected_nodes ?? {});
    const hasSelection = selectedNodes.length > 0;
    const hasClipboard = this.canvas.nodeClipboard.length > 0;

    const menu = document.createElement("div");
    menu.id = "zh-canvas-menu";
    menu.style.cssText = `
      position:fixed;z-index:10000;left:${event.clientX}px;top:${event.clientY}px;
      background:var(--toolbar-bg);border:1px solid var(--border);border-radius:4px;
      box-shadow:0 4px 16px rgba(0,0,0,0.4);font-family:sans-serif;font-size:13px;
      color:var(--text);min-width:170px;overflow:hidden;
    `;

    const makeItem = (label: string, enabled: boolean, onClick: () => void) => {
      const item = document.createElement("div");
      item.textContent = label;
      item.style.cssText = `padding:8px 14px;cursor:${enabled ? "pointer" : "default"};border-bottom:1px solid var(--border);color:${enabled ? "var(--text)" : "var(--text-dim)"};`;
      if (enabled) {
        item.addEventListener("mouseenter", () => { item.style.background = "var(--node-hover)"; });
        item.addEventListener("mouseleave", () => { item.style.background = ""; });
        item.addEventListener("click", () => { menu.remove(); onClick(); });
      }
      menu.appendChild(item);
    };

    makeItem("新建节点", true, () => this.canvas.onAddNodeRequest?.(graphX, graphY));
    makeItem("复制", hasSelection, () => this.copySelectedNodes());
    makeItem("粘贴", hasClipboard, () => { this.pasteNodes(graphX, graphY).catch(console.error); });
    const hasDeletable = selectedNodes.some((node: any) => {
      const id: string | undefined = node.zihuanId;
      return id !== "__function_inputs__" && id !== "__function_outputs__";
    });
    makeItem("删除", hasDeletable, () => { this.deleteSelectedNodes().catch(console.error); });
    const hasToggleable = selectedNodes.some((node: any) => {
      const id: string | undefined = node.zihuanId;
      return id !== undefined && id !== "__function_inputs__" && id !== "__function_outputs__";
    });
    const allDisabled = hasToggleable && selectedNodes.every((node: any) => {
      const id: string | undefined = node.zihuanId;
      if (!id || id === "__function_inputs__" || id === "__function_outputs__") return true;
      return !!this.canvas.state.graph?.nodes.find((item) => item.id === id)?.disabled;
    });
    makeItem(allDisabled ? "启用节点" : "禁用节点", hasToggleable, () => {
      this.toggleSelectedNodesDisabled().catch(console.error);
    });
    makeItem("提取为函数子图", hasSelection && this.canvas.state.graph !== null, () => {
      this.convertSelectionToFunction().catch(console.error);
    });

    document.body.appendChild(menu);
    const dismiss = (e: MouseEvent) => {
      if (!menu.contains(e.target as Node)) {
        menu.remove();
        document.removeEventListener("click", dismiss);
      }
    };
    setTimeout(() => document.addEventListener("click", dismiss), 0);
  }

  private copySelectedNodes(): void {
    const selectedNodes: any[] = Object.values((this.canvas.lCanvas as any).selected_nodes ?? {});
    if (selectedNodes.length === 0) return;
    const defs: NodeDefinition[] = [];
    for (const node of selectedNodes) {
      const nodeId = node.zihuanId as string | undefined;
      if (!nodeId) continue;
      const def = this.canvas.state.graph?.nodes.find((item) => item.id === nodeId);
      if (def) defs.push(def);
    }
    this.canvas.nodeClipboard = defs;
  }

  private async pasteNodes(graphX: number, graphY: number): Promise<void> {
    const sid = this.canvas.state.sessionId;
    if (!sid || this.canvas.nodeClipboard.length === 0) return;
    const xs = this.canvas.nodeClipboard.map((node) => node.position?.x ?? 0);
    const ys = this.canvas.nodeClipboard.map((node) => node.position?.y ?? 0);
    const minX = Math.min(...xs);
    const minY = Math.min(...ys);
    const offset = 20;
    for (const def of this.canvas.nodeClipboard) {
      const dx = (def.position?.x ?? 0) - minX;
      const dy = (def.position?.y ?? 0) - minY;
      await graphs.addNode(sid, def.node_type, def.name, graphX + dx + offset, graphY + dy + offset);
    }
    await this.canvas.reloadCurrentSession();
    this.canvas.state.dirty = true;
  }

  private async deleteSelectedNodes(): Promise<void> {
    const selectedNodes: any[] = Object.values((this.canvas.lCanvas as any).selected_nodes ?? {});
    if (selectedNodes.length === 0) return;
    for (const node of [...selectedNodes]) {
      const id: string | undefined = node.zihuanId;
      if (id === "__function_inputs__" || id === "__function_outputs__") continue;
      this.canvas.lGraph.remove(node);
    }
  }

  async toggleSelectedNodesDisabled(): Promise<void> {
    const sid = this.canvas.state.sessionId;
    const graph = this.canvas.state.graph;
    if (!sid || !graph) return;

    const selectedNodes: any[] = Object.values((this.canvas.lCanvas as any).selected_nodes ?? {});
    const targets: { id: string; nextDisabled: boolean }[] = [];
    for (const node of selectedNodes) {
      const id = node.zihuanId as string | undefined;
      if (!id) continue;
      if (id === "__function_inputs__" || id === "__function_outputs__") continue;
      const def = graph.nodes.find((item) => item.id === id);
      if (!def) continue;
      targets.push({ id, nextDisabled: !def.disabled });
    }
    if (targets.length === 0) return;

    await Promise.all(
      targets.map((target) => graphs.updateNode(sid, target.id, { disabled: target.nextDisabled })),
    );
    await this.canvas.reloadCurrentSession();
    this.canvas.state.dirty = true;
    this.canvas.onGraphDirty?.();
  }

  async showHPPicker(lNode: any, portName: string, event: MouseEvent): Promise<void> {
    const sid = this.canvas.state.sessionId;
    if (!sid) return;
    const hpSid = this.canvas.rootSessionId ?? sid;
    const { hyperparameters } = await graphs.getHyperparameters(hpSid);
    this.showBindingPicker(event, hyperparameters.map((item) => item.name), async (name) => {
      await graphs.updateNode(sid, lNode.zihuanId as string, {
        port_bindings: { [portName]: { kind: "hyperparameter", name } },
      });
      await this.canvas.reloadCurrentSession();
    });
  }

  async showVarPicker(lNode: any, portName: string, event: MouseEvent): Promise<void> {
    const sid = this.canvas.state.sessionId;
    if (!sid) return;
    const variables = await graphs.getVariables(sid);
    this.showBindingPicker(event, variables.map((item) => item.name), async (name) => {
      await graphs.updateNode(sid, lNode.zihuanId as string, {
        port_bindings: { [portName]: { kind: "variable", name } },
      });
      await this.canvas.reloadCurrentSession();
    });
  }

  showPortBindingMenu(lNode: any, _slotIndex: number, portName: string, event: MouseEvent): void {
    document.getElementById("zh-port-menu")?.remove();
    const menu = document.createElement("div");
    menu.id = "zh-port-menu";
    menu.style.cssText = `
      position:fixed;z-index:10000;left:${event.clientX}px;top:${event.clientY}px;
      background:#1a1a2e;border:1px solid #2a2a4a;border-radius:4px;
      box-shadow:0 4px 16px rgba(0,0,0,0.6);font-family:sans-serif;font-size:13px;
      color:#e0e0e0;min-width:170px;overflow:hidden;
    `;

    const makeItem = (label: string, onClick: () => void) => {
      const item = document.createElement("div");
      item.textContent = label;
      item.style.cssText = "padding:8px 14px;cursor:pointer;border-bottom:1px solid #1a2a4a;";
      item.addEventListener("mouseenter", () => { item.style.background = "#1a3a6e"; });
      item.addEventListener("mouseleave", () => { item.style.background = ""; });
      item.addEventListener("click", () => { menu.remove(); onClick(); });
      menu.appendChild(item);
    };

    makeItem("绑定超参数...", () => { this.showHPPicker(lNode, portName, event).catch(console.error); });
    makeItem("绑定变量...", () => { this.showVarPicker(lNode, portName, event).catch(console.error); });
    makeItem("清除绑定", () => { this.clearPortBinding(lNode, portName).catch(console.error); });

    document.body.appendChild(menu);
    const dismiss = (e: MouseEvent) => {
      if (!menu.contains(e.target as Node)) {
        menu.remove();
        document.removeEventListener("click", dismiss);
      }
    };
    setTimeout(() => document.addEventListener("click", dismiss), 0);
  }

  private showBindingPicker(
    event: MouseEvent,
    names: string[],
    onSelect: (name: string) => Promise<void>,
  ): void {
    document.getElementById("zh-port-picker")?.remove();
    const picker = document.createElement("div");
    picker.id = "zh-port-picker";
    picker.style.cssText = `
      position:fixed;z-index:10001;left:${event.clientX + 8}px;top:${event.clientY}px;
      background:#1a1a2e;border:1px solid #2a2a4a;border-radius:4px;
      box-shadow:0 4px 16px rgba(0,0,0,0.6);font-family:sans-serif;font-size:13px;
      color:#e0e0e0;min-width:160px;max-height:220px;overflow-y:auto;
    `;

    if (names.length === 0) {
      const empty = document.createElement("div");
      empty.textContent = "(无可用项)";
      empty.style.cssText = "padding:8px 14px;color:#888;";
      picker.appendChild(empty);
    }

    for (const name of names) {
      const item = document.createElement("div");
      item.textContent = name;
      item.style.cssText = "padding:8px 14px;cursor:pointer;border-bottom:1px solid #1a2a4a;";
      item.addEventListener("mouseenter", () => { item.style.background = "#1a3a6e"; });
      item.addEventListener("mouseleave", () => { item.style.background = ""; });
      item.addEventListener("click", () => { picker.remove(); onSelect(name).catch(console.error); });
      picker.appendChild(item);
    }

    document.body.appendChild(picker);
    const dismiss = (e: MouseEvent) => {
      if (!picker.contains(e.target as Node)) {
        picker.remove();
        document.removeEventListener("click", dismiss);
      }
    };
    setTimeout(() => document.addEventListener("click", dismiss), 0);
  }

  private async clearPortBinding(lNode: any, portName: string): Promise<void> {
    const sid = this.canvas.state.sessionId;
    if (!sid) return;
    const def = await graphs.get(sid);
    const nodeIdx = def.nodes.findIndex((node) => node.id === (lNode.zihuanId as string));
    if (nodeIdx < 0) return;
    const newBindings = { ...def.nodes[nodeIdx].port_bindings };
    delete newBindings[portName];
    const updatedGraph = {
      ...def,
      nodes: def.nodes.map((node, index) => index === nodeIdx ? { ...node, port_bindings: newBindings } : node),
    };
    await graphs.put(sid, updatedGraph);
    await this.canvas.reloadCurrentSession();
  }

  showNodeHelpDialog(lNode: any): void {
    const typeId: string = (lNode.constructor as any).zihuanTypeId ?? "";
    const typeInfo: NodeTypeInfo | undefined = getNodeTypeInfo(typeId);
    if (!typeInfo) return;

    const nodeId: string = lNode.zihuanId ?? "";
    const graph = this.canvas.state.graph;
    const inputConns = typeInfo.input_ports.filter((port) => !port.hidden).map((port) => {
      const connectedTo: Array<{ nodeName: string; portName: string }> = [];
      if (graph) {
        for (const edge of graph.edges) {
          if (edge.to_node_id === nodeId && edge.to_port === port.name) {
            const fromNode = graph.nodes.find((node) => node.id === edge.from_node_id);
            if (fromNode) connectedTo.push({ nodeName: fromNode.name, portName: edge.from_port });
          }
        }
      }
      return {
        portName: port.name,
        dataType: portTypeString(port.data_type),
        description: port.description,
        required: port.required,
        connectedTo,
      };
    });

    const outputConns = typeInfo.output_ports.map((port) => {
      const connectedTo: Array<{ nodeName: string; portName: string }> = [];
      if (graph) {
        for (const edge of graph.edges) {
          if (edge.from_node_id === nodeId && edge.from_port === port.name) {
            const toNode = graph.nodes.find((node) => node.id === edge.to_node_id);
            if (toNode) connectedTo.push({ nodeName: toNode.name, portName: edge.to_port });
          }
        }
      }
      return {
        portName: port.name,
        dataType: portTypeString(port.data_type),
        description: port.description,
        required: port.required,
        connectedTo,
      };
    });

    showNodeInfoDialog(typeInfo, inputConns, outputConns);
  }

  async convertSelectionToFunction(): Promise<void> {
    const sid = this.canvas.state.sessionId;
    const graph = this.canvas.state.graph;
    if (!sid || !graph) return;

    const selectedLNodes: any[] = Object.values((this.canvas.lCanvas as any).selected_nodes ?? {});
    const selectedIds = new Set<string>();
    for (const node of selectedLNodes) {
      const id = node.zihuanId as string | undefined;
      if (id) selectedIds.add(id);
    }
    if (selectedIds.size === 0) return;

    type ExtInEdge = { edge: EdgeDefinition; fnPortName: string };
    type ExtOutEdge = { edge: EdgeDefinition; fnPortName: string };

    const internalEdges: EdgeDefinition[] = [];
    const externalInEdges: ExtInEdge[] = [];
    const externalOutEdges: ExtOutEdge[] = [];
    const usedInPortNames = new Set<string>();
    const outPortKeyToName = new Map<string, string>();

    const safePortName = (preferred: string, usedSet: Set<string>, nodeName: string, portName: string): string => {
      if (!usedSet.has(preferred)) return preferred;
      const alt = `${nodeName.replace(/[^a-zA-Z0-9]+/g, "_")}_${portName}`;
      if (!usedSet.has(alt)) return alt;
      let i = 2;
      while (usedSet.has(`${alt}_${i}`)) i++;
      return `${alt}_${i}`;
    };

    const getPortDataType = (nodeId: string, portName: string, isOutput: boolean): string => {
      const nodeDef = graph.nodes.find((node) => node.id === nodeId);
      if (!nodeDef) return "Any";
      const ports = isOutput ? nodeDef.output_ports : nodeDef.input_ports;
      const port = ports.find((item) => item.name === portName);
      if (!port) return "Any";
      return portTypeString(port.data_type);
    };

    const getNodeDisplayName = (nodeId: string): string => graph.nodes.find((node) => node.id === nodeId)?.name ?? nodeId;

    for (const edge of graph.edges) {
      const fromSel = selectedIds.has(edge.from_node_id);
      const toSel = selectedIds.has(edge.to_node_id);
      if (fromSel && toSel) internalEdges.push(edge);
      else if (!fromSel && toSel) {
        const fnPortName = safePortName(edge.to_port, usedInPortNames, getNodeDisplayName(edge.to_node_id), edge.to_port);
        usedInPortNames.add(fnPortName);
        externalInEdges.push({ edge, fnPortName });
      } else if (fromSel && !toSel) {
        const key = `${edge.from_node_id}::${edge.from_port}`;
        if (!outPortKeyToName.has(key)) {
          const usedOut = new Set(outPortKeyToName.values());
          const fnPortName = safePortName(edge.from_port, usedOut, getNodeDisplayName(edge.from_node_id), edge.from_port);
          outPortKeyToName.set(key, fnPortName);
        }
        externalOutEdges.push({ edge, fnPortName: outPortKeyToName.get(key)! });
      }
    }

    const fnInputPortDefs = externalInEdges.map((item) => ({
      name: item.fnPortName,
      data_type: getPortDataType(item.edge.from_node_id, item.edge.from_port, true),
    }));

    const seenOutNames = new Set<string>();
    const fnOutputPortDefs: Array<{ name: string; data_type: string }> = [];
    for (const item of externalOutEdges) {
      if (seenOutNames.has(item.fnPortName)) continue;
      seenOutNames.add(item.fnPortName);
      fnOutputPortDefs.push({
        name: item.fnPortName,
        data_type: getPortDataType(item.edge.from_node_id, item.edge.from_port, true),
      });
    }

    const selectedDefs = graph.nodes.filter((node) => selectedIds.has(node.id));
    const xs = selectedDefs.map((node) => node.position?.x ?? 0);
    const ys = selectedDefs.map((node) => node.position?.y ?? 0);
    const centroidX = xs.reduce((sum, value) => sum + value, 0) / (xs.length || 1);
    const centroidY = ys.reduce((sum, value) => sum + value, 0) / (ys.length || 1);
    const minX = Math.min(...xs);
    const maxX = Math.max(...xs);
    const inputsBoundaryX = minX - 300;
    const outputsBoundaryX = maxX + 300;
    const boundaryY = centroidY;

    const fnInputsNode: NodeDefinition = {
      id: "__function_inputs__",
      name: "函数输入",
      description: "函数子图的输入边界节点",
      node_type: "function_inputs",
      input_ports: [
        { name: "signature", data_type: "Json", description: null, required: false },
        { name: "runtime_values", data_type: "Json", description: null, required: false },
      ],
      output_ports: fnInputPortDefs.map((port) => ({ ...port, description: null, required: false })),
      dynamic_input_ports: false,
      dynamic_output_ports: true,
      position: { x: inputsBoundaryX, y: boundaryY },
      size: { width: 220, height: 120 },
      inline_values: { signature: fnInputPortDefs },
      port_bindings: {},
      has_error: false,
      has_cycle: false,
    };

    const fnOutputsNode: NodeDefinition = {
      id: "__function_outputs__",
      name: "函数输出",
      description: "函数子图的输出边界节点",
      node_type: "function_outputs",
      input_ports: [
        { name: "signature", data_type: "Json", description: null, required: false },
        ...fnOutputPortDefs.map((port) => ({ ...port, description: null, required: false })),
      ],
      output_ports: [],
      dynamic_input_ports: true,
      dynamic_output_ports: false,
      position: { x: outputsBoundaryX, y: boundaryY },
      size: { width: 220, height: 120 },
      inline_values: { signature: fnOutputPortDefs },
      port_bindings: {},
      has_error: false,
      has_cycle: false,
    };

    const subgraphEdges: EdgeDefinition[] = [
      ...internalEdges,
      ...externalInEdges.map((item) => ({
        from_node_id: "__function_inputs__",
        from_port: item.fnPortName,
        to_node_id: item.edge.to_node_id,
        to_port: item.edge.to_port,
      })),
    ];
    const addedOutKeys = new Set<string>();
    for (const item of externalOutEdges) {
      const key = `${item.edge.from_node_id}::${item.edge.from_port}`;
      if (addedOutKeys.has(key)) continue;
      addedOutKeys.add(key);
      subgraphEdges.push({
        from_node_id: item.edge.from_node_id,
        from_port: item.edge.from_port,
        to_node_id: "__function_outputs__",
        to_port: item.fnPortName,
      });
    }

    const functionConfig = {
      name: "New Function",
      description: "",
      inputs: fnInputPortDefs,
      outputs: fnOutputPortDefs,
      subgraph: {
        nodes: [...selectedDefs, fnInputsNode, fnOutputsNode],
        edges: subgraphEdges,
        hyperparameter_groups: [],
        hyperparameters: [],
        variables: [],
        metadata: { name: null, description: null, version: null },
      } as NodeGraphDefinition,
    };

    const fnNodeId = crypto.randomUUID();
    const functionNode: NodeDefinition = {
      id: fnNodeId,
      name: "New Function",
      description: null,
      node_type: "function",
      input_ports: fnInputPortDefs.map((port) => ({ ...port, description: null, required: false })),
      output_ports: fnOutputPortDefs.map((port) => ({ ...port, description: null, required: false })),
      dynamic_input_ports: true,
      dynamic_output_ports: true,
      position: { x: centroidX, y: centroidY },
      size: { width: 220, height: 80 + Math.max(fnInputPortDefs.length, fnOutputPortDefs.length) * 20 },
      inline_values: { function_config: functionConfig as unknown as Record<string, unknown> },
      port_bindings: {},
      has_error: false,
      has_cycle: false,
    };

    const newGraph: NodeGraphDefinition = {
      ...graph,
      nodes: [...graph.nodes.filter((node) => !selectedIds.has(node.id)), functionNode],
      edges: [
        ...graph.edges.filter((edge) => !selectedIds.has(edge.from_node_id) && !selectedIds.has(edge.to_node_id)),
        ...externalInEdges.map((item) => ({
          from_node_id: item.edge.from_node_id,
          from_port: item.edge.from_port,
          to_node_id: fnNodeId,
          to_port: item.fnPortName,
        })),
        ...externalOutEdges.map((item) => ({
          from_node_id: fnNodeId,
          from_port: item.fnPortName,
          to_node_id: item.edge.to_node_id,
          to_port: item.edge.to_port,
        })),
      ],
    };

    await graphs.put(sid, newGraph);
    this.canvas.history.push(newGraph);
    this.canvas.onHistoryChange?.();
    await this.canvas.reloadCurrentSession();
    this.canvas.state.dirty = true;
    this.canvas.onGraphDirty?.();
  }
}
