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
const BLOCK_GAP = 6;
const MAX_HEIGHT = 600;
const MAX_IMAGE_DIM = 200;
const PLACEHOLDER_HEIGHT = 60;

const BUBBLE_PAD_X = 10;
const BUBBLE_PAD_Y = 6;
const BUBBLE_RADIUS = 10;
const BUBBLE_BG_TEXT = "#3a3a3a";
const BUBBLE_BG_AT = "#1f3a55";
const BUBBLE_BG_REPLY = "#3a2a48";
const BUBBLE_BG_FORWARD = "#4a3820";
const BUBBLE_BG_ERROR = "#4a2828";
const BUBBLE_BG_IMAGE = "#2c2c2c";
const BUBBLE_BORDER = "rgba(255,255,255,0.06)";

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

function roundRectPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  const radius = Math.max(0, Math.min(r, w / 2, h / 2));
  ctx.beginPath();
  if (typeof (ctx as any).roundRect === "function") {
    (ctx as any).roundRect(x, y, w, h, radius);
    return;
  }
  ctx.moveTo(x + radius, y);
  ctx.lineTo(x + w - radius, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + radius);
  ctx.lineTo(x + w, y + h - radius);
  ctx.quadraticCurveTo(x + w, y + h, x + w - radius, y + h);
  ctx.lineTo(x + radius, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - radius);
  ctx.lineTo(x, y + radius);
  ctx.quadraticCurveTo(x, y, x + radius, y);
  ctx.closePath();
}

function fillBubble(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  bg: string,
): void {
  ctx.save();
  ctx.fillStyle = bg;
  roundRectPath(ctx, x, y, w, h, BUBBLE_RADIUS);
  ctx.fill();
  ctx.lineWidth = 1;
  ctx.strokeStyle = BUBBLE_BORDER;
  ctx.stroke();
  ctx.restore();
}

function drawPlaceholderBubble(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  label: string,
): void {
  fillBubble(ctx, x, y, w, h, BUBBLE_BG_IMAGE);
  ctx.save();
  ctx.fillStyle = "#999";
  ctx.font = "12px sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(label, x + w / 2, y + h / 2);
  ctx.restore();
}

function drawTextBubble(
  ctx: CanvasRenderingContext2D,
  prefix: string,
  body: string,
  x: number,
  y: number,
  maxWidth: number,
  prefixColor: string,
  bubbleBg: string,
): number {
  ctx.font = "12px sans-serif";
  ctx.textAlign = "left";
  ctx.textBaseline = "top";

  const contentMaxWidth = Math.max(20, maxWidth - BUBBLE_PAD_X * 2);
  const lines: { text: string; color: string }[] = [];
  if (prefix) lines.push({ text: prefix, color: prefixColor });
  if (body) {
    const wrapped = wrapText(ctx, body, contentMaxWidth);
    for (const line of wrapped) lines.push({ text: line, color: "#e8e8e8" });
  }
  if (lines.length === 0) return 0;

  let widest = 0;
  for (const line of lines) {
    const w = ctx.measureText(line.text).width;
    if (w > widest) widest = w;
  }
  const bubbleW = Math.min(maxWidth, Math.ceil(widest) + BUBBLE_PAD_X * 2);
  const bubbleH = lines.length * LINE_HEIGHT + BUBBLE_PAD_Y * 2;

  fillBubble(ctx, x, y, bubbleW, bubbleH, bubbleBg);

  let cursorY = y + BUBBLE_PAD_Y;
  for (const line of lines) {
    ctx.fillStyle = line.color;
    ctx.fillText(line.text, x + BUBBLE_PAD_X, cursorY);
    cursorY += LINE_HEIGHT;
  }
  return bubbleH;
}

function drawImageBubble(
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
    drawPlaceholderBubble(ctx, x, y, boxW, PLACEHOLDER_HEIGHT, "（无图片源）");
    return PLACEHOLDER_HEIGHT;
  }
  const entry = getOrCreateImage(url, lNode);
  if (entry.failed) {
    drawPlaceholderBubble(ctx, x, y, boxW, PLACEHOLDER_HEIGHT, "图像加载失败");
    return PLACEHOLDER_HEIGHT;
  }
  const img = entry.img;
  if (!img.complete || img.naturalWidth === 0) {
    drawPlaceholderBubble(ctx, x, y, boxW, PLACEHOLDER_HEIGHT, "加载中…");
    return PLACEHOLDER_HEIGHT;
  }
  const scale = Math.min(boxW / img.naturalWidth, MAX_IMAGE_DIM / img.naturalHeight, 1);
  const drawW = Math.max(1, Math.round(img.naturalWidth * scale));
  const drawH = Math.max(1, Math.round(img.naturalHeight * scale));

  fillBubble(ctx, x, y, drawW, drawH, BUBBLE_BG_IMAGE);

  ctx.save();
  roundRectPath(ctx, x, y, drawW, drawH, BUBBLE_RADIUS);
  ctx.clip();
  try {
    ctx.drawImage(img, x, y, drawW, drawH);
  } catch {
    ctx.restore();
    drawPlaceholderBubble(ctx, x, y, boxW, PLACEHOLDER_HEIGHT, "图像加载失败");
    return PLACEHOLDER_HEIGHT;
  }
  ctx.restore();
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
        blockHeight = drawTextBubble(
          ctx,
          "",
          msg.data.text ?? "",
          x,
          y,
          maxWidth,
          "#aaa",
          BUBBLE_BG_TEXT,
        );
        break;
      case "at":
        blockHeight = drawTextBubble(
          ctx,
          `@${msg.data.target ?? "?"}`,
          "",
          x,
          y,
          maxWidth,
          "#79c8ff",
          BUBBLE_BG_AT,
        );
        break;
      case "reply":
        blockHeight = drawTextBubble(
          ctx,
          `[Reply id=${msg.data.id ?? "?"}]`,
          "",
          x,
          y,
          maxWidth,
          "#dba8e0",
          BUBBLE_BG_REPLY,
        );
        break;
      case "forward": {
        const count = msg.data.content?.length ?? 0;
        blockHeight = drawTextBubble(
          ctx,
          `[Forward (${count})]`,
          "",
          x,
          y,
          maxWidth,
          "#ffc97a",
          BUBBLE_BG_FORWARD,
        );
        break;
      }
      case "image":
        blockHeight = drawImageBubble(ctx, msg, x, y, maxWidth, lNode);
        break;
      default:
        blockHeight = drawTextBubble(
          ctx,
          `[未知类型: ${(msg as any).type}]`,
          "",
          x,
          y,
          maxWidth,
          "#ef9a9a",
          BUBBLE_BG_ERROR,
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
