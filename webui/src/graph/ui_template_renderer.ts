import type { NodeDefinition, NodeTypeInfo, ServerMessage, JsonValue } from "../api/types";
import type { ZihuanWS } from "../api/ws";

type TemplateNode = HTMLElement & { __zihuanNodeId?: string };

function valueAt(state: JsonValue | null | undefined, path: string): unknown {
  return path.split(".").reduce<unknown>((value, key) => {
    if (!value || typeof value !== "object") return undefined;
    return (value as Record<string, unknown>)[key];
  }, state);
}

function safeTemplate(html: string): DocumentFragment {
  const template = document.createElement("template");
  template.innerHTML = html;
  template.content.querySelectorAll("script,iframe,object,embed").forEach((node) => node.remove());
  template.content.querySelectorAll("*").forEach((node) => {
    for (const attr of Array.from(node.attributes)) {
      if (attr.name.toLowerCase().startsWith("on") || attr.value.toLowerCase().startsWith("javascript:")) node.removeAttribute(attr.name);
    }
  });
  return template.content;
}

function bind(root: TemplateNode, state: JsonValue, send: (nodeId: string, event: string, payload: JsonValue) => void): void {
  root.querySelectorAll<HTMLElement>("[data-bind-if]").forEach((node) => {
    node.hidden = !Boolean(valueAt(state, node.dataset.bindIf ?? ""));
  });
  root.querySelectorAll<HTMLElement>("[data-bind-text]").forEach((node) => {
    const value = valueAt(state, node.dataset.bindText ?? "");
    node.textContent = value == null ? "" : String(value);
  });
  root.querySelectorAll<HTMLElement>("[data-bind-attr]").forEach((node) => {
    const binding = node.dataset.bindAttr ?? "";
    const separator = binding.indexOf(":");
    if (separator < 1) return;
    const attr = binding.slice(0, separator);
    const value = valueAt(state, binding.slice(separator + 1));
    if (value == null || value === false) node.removeAttribute(attr);
    else node.setAttribute(attr, String(value));
  });
  root.querySelectorAll<HTMLElement>("[data-bind-each]").forEach((node) => {
    const items = valueAt(state, node.dataset.bindEach ?? "");
    if (!Array.isArray(items)) return;
    const template = node.firstElementChild?.cloneNode(true);
    if (!template) return;
    node.replaceChildren();
    for (const item of items) {
      const child = template.cloneNode(true) as TemplateNode;
      bind(child, (item ?? null) as JsonValue, send);
      node.appendChild(child);
    }
  });
  root.querySelectorAll<HTMLElement>("[data-ui-event]").forEach((node) => {
    const event = node.dataset.uiEvent;
    if (!event || node.dataset.zihuanBound === "1") return;
    node.dataset.zihuanBound = "1";
    node.addEventListener("click", () => {
      const nodeId = root.__zihuanNodeId;
      if (nodeId) send(nodeId, event, (node as HTMLInputElement).value as JsonValue);
    });
  });
}

export class NodeUiTemplateRenderer {
  private readonly states = new Map<string, JsonValue>();
  private readonly tasks = new Map<string, string>();
  private readonly elements = new Map<string, TemplateNode>();
  private readonly container: HTMLElement;
  private readonly unsubscribe: () => void;

  constructor(private readonly canvas: any, socket: ZihuanWS) {
    this.container = (canvas.lCanvas as any).canvas?.parentElement ?? document.body;
    this.unsubscribe = socket.onMessage((message: ServerMessage) => {
      if (message.type !== "NodeUiUpdate") return;
      this.tasks.set(message.node_id, message.task_id);
      this.states.set(message.node_id, message.state);
      this.renderNode(message.node_id);
    });
    window.addEventListener("resize", () => this.reposition());
  }

  clear(): void { for (const element of this.elements.values()) element.remove(); this.elements.clear(); }
  dispose(): void { this.unsubscribe(); this.clear(); }

  mount(node: any, definition: NodeDefinition, info: NodeTypeInfo | undefined): void {
    if (!info?.ui_template) return;
    const element = document.createElement("div") as TemplateNode;
    element.className = "zihuan-node-ui-template";
    element.__zihuanNodeId = definition.id;
    element.dataset.nodeId = definition.id;
    element.appendChild(safeTemplate(info.ui_template));
    this.container.appendChild(element);
    this.elements.set(definition.id, element);
    this.position(definition.id);
    bind(element, this.states.get(definition.id) ?? (definition.ui_state as JsonValue ?? null), (nodeId, event, payload) => {
      const socket = (window as any).__zihuanWs as ZihuanWS | undefined;
      socket?.send({ type: "NodeUiEvent", graph_session_id: this.canvas.state.sessionId ?? "", task_id: this.tasks.get(nodeId), node_id: nodeId, event, payload });
    });
  }

  private renderNode(nodeId: string): void {
    const element = this.elements.get(nodeId);
    if (!element) return;
    bind(element, this.states.get(nodeId) ?? null, (id, event, payload) => {
      const socket = (window as any).__zihuanWs;
      socket?.send({ type: "NodeUiEvent", graph_session_id: this.canvas.state.sessionId ?? "", task_id: this.tasks.get(id), node_id: id, event, payload });
    });
  }

  reposition(): void { for (const id of this.elements.keys()) this.position(id); }

  private position(nodeId: string): void {
    const node = this.canvas.nodeMap.get(nodeId);
    const element = this.elements.get(nodeId);
    if (!node || !element) return;
    const canvasElement = (this.canvas.lCanvas as any).canvas as HTMLCanvasElement;
    const rect = canvasElement.getBoundingClientRect();
    const ds = (this.canvas.lCanvas as any).ds;
    const scale = ds?.scale ?? 1;
    const left = rect.left + (node.pos[0] + ds.offset[0]) * scale;
    const top = rect.top + (node.pos[1] + 30 + ds.offset[1]) * scale;
    element.style.left = `${left}px`;
    element.style.top = `${top}px`;
    element.style.width = `${Math.max(40, node.size[0] * scale - 16 * scale)}px`;
    element.style.transform = `scale(${scale})`;
    element.style.transformOrigin = "top left";
  }
}
