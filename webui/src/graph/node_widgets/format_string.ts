import type { NodeDefinition } from "../../api/types";
import { openFormatStringEditor } from "../../ui/dialogs/index";

const PADDING = 8;
const LINE_HEIGHT = 16;
const SLOT_HEIGHT = 20;

export function setupFormatStringWidgets(
  lNode: any,
  nodeDef: NodeDefinition,
  getSessionId: () => string | null,
  onRefresh: () => void
): void {
  let template = (nodeDef.inline_values?.["template"] as string) ?? "";

  const prev = lNode.onDrawForeground;
  lNode.onDrawForeground = function (this: any, ctx: CanvasRenderingContext2D) {
    if (typeof prev === "function") prev.call(this, ctx);

    const nodeWidth: number = this.size?.[0] ?? 200;
    const maxTextWidth = Math.max(20, nodeWidth - PADDING * 2);
    const slotCount = Math.max(this.inputs?.length ?? 0, this.outputs?.length ?? 0);
    const startY = slotCount * SLOT_HEIGHT + PADDING;

    ctx.save();
    ctx.font = "12px monospace";
    ctx.textAlign = "left";
    ctx.textBaseline = "top";

    const display = template || "(空模板 — 双击编辑)";
    const lines: string[] = [];
    for (const para of display.split("\n")) {
      if (para.length === 0) {
        lines.push("");
        continue;
      }
      let current = "";
      for (const ch of para) {
        const next = current + ch;
        if (ctx.measureText(next).width <= maxTextWidth) {
          current = next;
        } else {
          if (current.length > 0) lines.push(current);
          current = ch;
        }
      }
      if (current.length > 0) lines.push(current);
    }
    if (lines.length === 0) lines.push("");

    const boxH = lines.length * LINE_HEIGHT + PADDING;
    const bx = PADDING / 2;
    const bw = nodeWidth - PADDING;

    ctx.fillStyle = "rgba(0,0,0,0.25)";
    ctx.strokeStyle = "rgba(255,255,255,0.08)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.rect(bx, startY, bw, boxH);
    ctx.fill();
    ctx.stroke();

    ctx.fillStyle = template ? "#d4e0f0" : "#777";
    let curY = startY + PADDING / 2;
    for (const line of lines) {
      ctx.fillText(line, PADDING, curY);
      curY += LINE_HEIGHT;
    }

    ctx.restore();

    const desired = startY + boxH + PADDING;
    if (this.size && this.size[1] < desired) {
      this.size[1] = desired;
    }
  };

  lNode.onDblClick = (_e: MouseEvent) => {
    const sid = getSessionId();
    if (!sid) {
      alert("请先打开一个图。");
      return;
    }
    openFormatStringEditor(nodeDef, sid, () => {
      template = (nodeDef.inline_values?.["template"] as string) ?? "";
      onRefresh();
    });
  };
}
