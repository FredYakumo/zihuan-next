// Live preview of Vec<QQMessage> rendered on the qq_message_preview node card.
// Listens for NodePreviewQQMessages WS frames and draws messages (text + images)
// onto the node via canvas onDrawForeground, while preserving any existing
// onDrawForeground assigned upstream (binding badges, help button).

import type { NodeDefinition } from "../api/types";
import type { ServerMessage } from "../api/types";
import type { QQMessageItem } from "../ui/dialogs/types";
import type { ZihuanWS } from "../api/ws";

const PADDING_X = 8;
const PADDING_TOP = 30;
const LINE_HEIGHT = 16;
const BLOCK_GAP = 8;
const MAX_HEIGHT = 600;
const MAX_IMAGE_DIM = 200;
const PLACEHOLDER_HEIGHT = 60;

// keyed by nodeDef.id (node ids are unique within the running webui)
const previewStore: Map<string, QQMessageItem[]> = new Map();
const nodeRefs: Map<string, any> = new Map();
type CachedImage = { img: HTMLImageElement; failed: boolean };
const imageCache: Map<string, CachedImage> = new Map();

let wsHandlerInstalled = false;

function dirty(lNode: any): void {
  const graph = lNode.graph;
  if (graph && typeof graph.setDirtyCanvas === "function") {
    graph.setDirtyCanvas(true, true);
  } else if (typeof lNode.setDirtyCanvas === "function") {
    lNode.setDirtyCanvas(true, true);
  }
}

function pickImageUrl(msg: QQMessageItem): string {
  return msg.data.object_url || msg.data.url || msg.data.path || msg.data.file || "";
}

function getOrCreateImage(url: string, lNode: any): CachedImage {
  const cached = imageCache.get(url);
  if (cached) return cached;
  const img = new Image();
  img.crossOrigin = "anonymous";
  const entry: CachedImage = { img, failed: false };
  imageCache.set(url, entry);
  img.onload = () => dirty(lNode);
  img.onerror = () => {
    entry.failed = true;
    dirty(lNode);
  };
  img.src = url;
  return entry;
}

function wrapText(
  ctx: CanvasRenderingContext2D,
  text: string,
  maxWidth: number,
): string[] {
  if (!text) return [""];
  const lines: string[] = [];
  const paragraphs = text.split("\n");
  for (const para of paragraphs) {
    if (para.length === 0) {
      lines.push("");
      continue;
    }
    let current = "";
    for (const ch of para) {
      const next = current + ch;
      if (ctx.measureText(next).width <= maxWidth) {
        current = next;
      } else {
        if (current.length > 0) lines.push(current);
        current = ch;
      }
    }
    if (current.length > 0) lines.push(current);
  }
  return lines.length > 0 ? lines : [""];
}

function drawPlaceholderBox(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  label: string,
): void {
  ctx.save();
  ctx.fillStyle = "#2a2a2a";
  ctx.strokeStyle = "#555";
  ctx.lineWidth = 1;
  ctx.fillRect(x, y, w, h);
  ctx.strokeRect(x, y, w, h);
  ctx.fillStyle = "#999";
  ctx.font = "12px sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(label, x + w / 2, y + h / 2);
  ctx.restore();
}

function drawTextBlock(
  ctx: CanvasRenderingContext2D,
  prefix: string,
  body: string,
  x: number,
  y: number,
  maxWidth: number,
  prefixColor: string,
): number {
  ctx.font = "12px sans-serif";
  ctx.textAlign = "left";
  ctx.textBaseline = "top";

  let cursorY = y;
  if (prefix) {
    ctx.fillStyle = prefixColor;
    ctx.fillText(prefix, x, cursorY);
    cursorY += LINE_HEIGHT;
  }
  if (body) {
    ctx.fillStyle = "#e0e0e0";
    const lines = wrapText(ctx, body, maxWidth);
    for (const line of lines) {
      ctx.fillText(line, x, cursorY);
      cursorY += LINE_HEIGHT;
    }
  }
  return cursorY - y;
}

function drawImageBlock(
  ctx: CanvasRenderingContext2D,
  msg: QQMessageItem,
  x: number,
  y: number,
  maxWidth: number,
  lNode: any,
): number {
  const url = pickImageUrl(msg);
  const boxW = Math.min(maxWidth, MAX_IMAGE_DIM);
  if (!url) {
    drawPlaceholderBox(ctx, x, y, boxW, PLACEHOLDER_HEIGHT, "（无图片源）");
    return PLACEHOLDER_HEIGHT;
  }
  const entry = getOrCreateImage(url, lNode);
  if (entry.failed) {
    drawPlaceholderBox(ctx, x, y, boxW, PLACEHOLDER_HEIGHT, "图像加载失败");
    return PLACEHOLDER_HEIGHT;
  }
  const img = entry.img;
  if (!img.complete || img.naturalWidth === 0) {
    drawPlaceholderBox(ctx, x, y, boxW, PLACEHOLDER_HEIGHT, "加载中…");
    return PLACEHOLDER_HEIGHT;
  }
  const scale = Math.min(boxW / img.naturalWidth, MAX_IMAGE_DIM / img.naturalHeight, 1);
  const drawW = Math.max(1, Math.round(img.naturalWidth * scale));
  const drawH = Math.max(1, Math.round(img.naturalHeight * scale));
  try {
    ctx.drawImage(img, x, y, drawW, drawH);
  } catch {
    drawPlaceholderBox(ctx, x, y, boxW, PLACEHOLDER_HEIGHT, "图像加载失败");
    return PLACEHOLDER_HEIGHT;
  }
  return drawH;
}

function drawPreview(
  ctx: CanvasRenderingContext2D,
  lNode: any,
  messages: QQMessageItem[],
): void {
  const nodeWidth: number = lNode.size?.[0] ?? 240;
  const maxWidth = Math.max(40, nodeWidth - PADDING_X * 2);
  const x = PADDING_X;
  let y = PADDING_TOP;

  ctx.save();
  ctx.font = "12px sans-serif";
  ctx.textAlign = "left";
  ctx.textBaseline = "top";

  if (messages.length === 0) {
    ctx.fillStyle = "#888";
    ctx.fillText("（暂无消息，运行图后将在此显示）", x, y);
    y += LINE_HEIGHT;
    ctx.restore();
    autoSize(lNode, y);
    return;
  }

  let truncated = 0;
  for (let i = 0; i < messages.length; i++) {
    if (y > MAX_HEIGHT - LINE_HEIGHT) {
      truncated = messages.length - i;
      break;
    }
    const msg = messages[i];
    let blockHeight = 0;
    switch (msg.type) {
      case "text":
        blockHeight = drawTextBlock(
          ctx,
          "",
          msg.data.text ?? "",
          x,
          y,
          maxWidth,
          "#aaa",
        );
        break;
      case "at":
        blockHeight = drawTextBlock(
          ctx,
          `@${msg.data.target ?? "?"}`,
          "",
          x,
          y,
          maxWidth,
          "#4fc3f7",
        );
        break;
      case "reply":
        blockHeight = drawTextBlock(
          ctx,
          `[Reply id=${msg.data.id ?? "?"}]`,
          "",
          x,
          y,
          maxWidth,
          "#ce93d8",
        );
        break;
      case "forward": {
        const count = msg.data.content?.length ?? 0;
        blockHeight = drawTextBlock(
          ctx,
          `[Forward (${count})]`,
          "",
          x,
          y,
          maxWidth,
          "#ffb74d",
        );
        break;
      }
      case "image":
        blockHeight = drawImageBlock(ctx, msg, x, y, maxWidth, lNode);
        break;
      default:
        blockHeight = drawTextBlock(
          ctx,
          `[未知类型: ${(msg as any).type}]`,
          "",
          x,
          y,
          maxWidth,
          "#e57373",
        );
        break;
    }
    y += blockHeight + BLOCK_GAP;
  }

  if (truncated > 0) {
    ctx.fillStyle = "#888";
    ctx.fillText(`… 还有 ${truncated} 条`, x, y);
    y += LINE_HEIGHT;
  }
  ctx.restore();
  autoSize(lNode, y);
}

function autoSize(lNode: any, contentBottom: number): void {
  const desired = Math.min(MAX_HEIGHT, contentBottom + PADDING_X);
  if (lNode.size && lNode.size[1] < desired) {
    lNode.size[1] = desired;
    if (typeof lNode.setSize === "function") {
      lNode.setSize(lNode.size);
    }
  }
}

export function setupQQMessagePreviewWidgets(lNode: any, nodeDef: NodeDefinition): void {
  nodeRefs.set(nodeDef.id, lNode);

  if (!lNode.size || lNode.size[1] < 80) {
    lNode.size = [Math.max(lNode.size?.[0] ?? 240, 240), 80];
  }

  const prev = lNode.onDrawForeground;
  lNode.onDrawForeground = function (this: any, ctx: CanvasRenderingContext2D) {
    if (typeof prev === "function") prev.call(this, ctx);
    const messages = previewStore.get(nodeDef.id) ?? [];
    drawPreview(ctx, this, messages);
  };
}

function lookupNode(nodeId: string): any | null {
  return nodeRefs.get(nodeId) ?? null;
}

export function installPreviewWsHandler(socket: ZihuanWS): void {
  if (wsHandlerInstalled) return;
  wsHandlerInstalled = true;
  socket.onMessage((msg: ServerMessage) => {
    if (msg.type === "TaskStarted") {
      previewStore.clear();
      for (const [, node] of nodeRefs) {
        if (node) dirty(node);
      }
      return;
    }
    if (msg.type === "NodePreviewQQMessages") {
      const messages = Array.isArray(msg.messages) ? msg.messages : [];
      previewStore.set(msg.node_id, messages);
      const node = lookupNode(msg.node_id);
      if (node) dirty(node);
    }
  });
}
