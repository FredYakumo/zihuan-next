import { LGraphCanvas, LiteGraph } from "litegraph.js";
import { computeLinkGeometry, pointOnLinkGeometry, traceLinkPath } from "../link_layout";
import { getLiteGraphColors } from "../../ui/theme";
import { truncateText } from "./rendering";

let installed = false;

export function installLiteGraphPatches(): void {
  if (installed) return;
  installed = true;

  const originalIsValidConnection = (LiteGraph as any).isValidConnection.bind(LiteGraph);
  (LiteGraph as any).isValidConnection = function (typeA: unknown, typeB: unknown): boolean {
    if (
      typeA === "Any" || typeB === "Any" ||
      (typeof typeA === "string" && typeA.toLowerCase() === "any") ||
      (typeof typeB === "string" && typeB.toLowerCase() === "any")
    ) return true;
    if (typeof typeA === "string" && typeof typeB === "string") {
      const isVecAny = (t: string) => /^Vec<Any>$/i.test(t);
      const isVec = (t: string) => /^Vec<.+>/i.test(t);
      if ((isVecAny(typeA) && isVec(typeB)) || (isVecAny(typeB) && isVec(typeA))) return true;
    }
    return originalIsValidConnection(typeA, typeB);
  };

  const originalRenderLink = (LGraphCanvas.prototype as any).renderLink;
  (LGraphCanvas.prototype as any).renderLink = function (
    ctx: CanvasRenderingContext2D,
    a: any,
    b: any,
    link: any,
    skipBorder: boolean,
    flow: boolean,
    color: string,
    startDir: number,
    endDir: number,
    numSublines: number,
  ) {
    const shouldUseCustomRenderer = !!link && (!numSublines || numSublines <= 1);
    if (!shouldUseCustomRenderer) {
      return originalRenderLink.call(this, ctx, a, b, link, skipBorder, flow, color, startDir, endDir, numSublines);
    }

    const colors = getLiteGraphColors();
    this.visible_links.push(link);
    if (!color && link) {
      color = link.color || (LGraphCanvas as any).link_type_colors?.[link.type];
    }
    if (!color) color = this.default_link_color;
    if (link != null && this.highlighted_links?.[link.id]) color = colors.highlightedLinkColor;

    const fanout = getLinkFanoutInfo(this, link);
    const geometry = computeLinkGeometry(
      { x: a[0], y: a[1] },
      { x: b[0], y: b[1] },
      fanout.index,
      fanout.count,
    );

    if (!link._pos) link._pos = new Float32Array(2);
    link._pos[0] = geometry.midPoint.x;
    link._pos[1] = geometry.midPoint.y;
    link._zhLabelPos = [geometry.labelAnchor.x, geometry.labelAnchor.y];
    link._zhFanoutCount = fanout.count;
    const mainWidth = Math.max(2.6, (this.connections_width ?? 3) - 0.05);
    const haloWidth = mainWidth + 3.2;
    ctx.save();
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    if (!skipBorder && (this.ds?.scale ?? 1) > 0.5) {
      ctx.beginPath();
      traceLinkPath(ctx, geometry);
      ctx.lineWidth = haloWidth;
      ctx.strokeStyle = colors.linkHalo;
      ctx.stroke();
    }

    ctx.beginPath();
    traceLinkPath(ctx, geometry);
    ctx.lineWidth = mainWidth;
    ctx.strokeStyle = color;
    ctx.stroke();

    if (flow) {
      ctx.fillStyle = color;
      const dotRadius = Math.max(2.6, mainWidth * 0.9);
      const now = typeof (LiteGraph as any).getTime === "function" ? (LiteGraph as any).getTime() : Date.now();
      for (let i = 0; i < 5; i++) {
        const t = (now * 0.001 + i * 0.18) % 1;
        const pos = pointOnLinkGeometry(geometry, t);
        ctx.beginPath();
        ctx.arc(pos.x, pos.y, dotRadius, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    ctx.restore();
    return;
  };

  const originalDrawNodeShape = (LGraphCanvas.prototype as any).drawNodeShape;
  (LGraphCanvas.prototype as any).drawNodeShape = function (
    node: any,
    ctx: CanvasRenderingContext2D,
    size: any,
    fgColor: string,
    bgColor: string,
    selected: boolean,
    mouseOver: boolean,
  ) {
    if (node.onDrawTitleText) {
      const originalGetTitle = node.getTitle;
      node.getTitle = () => "";
      originalDrawNodeShape.call(this, node, ctx, size, fgColor, bgColor, selected, mouseOver);
      node.getTitle = originalGetTitle;
    } else {
      originalDrawNodeShape.call(this, node, ctx, size, fgColor, bgColor, selected, mouseOver);
    }
    const desc: string = node._functionDescription ?? "";
    const descHeight: number = node._descHeight ?? 0;
    if (desc && descHeight > 0) {
      const colors = getLiteGraphColors();
      ctx.save();
      ctx.fillStyle = node.color ?? colors.nodeHeader;
      ctx.globalAlpha = 0.85;
      ctx.fillRect(0, 0, size[0], descHeight);
      ctx.globalAlpha = 0.8;
      ctx.font = "italic 11px Arial";
      ctx.textAlign = "left";
      ctx.textBaseline = "middle";
      ctx.fillStyle = colors.nodeTitleText;
      const maxWidth = size[0] - 12;
      const truncated = truncateText(ctx, desc, maxWidth);
      ctx.fillText(truncated, 6, descHeight / 2);
      ctx.restore();
    }
  };

  // Tighten port hit zones to prevent accidental connection drags when clicking
  // on the node body near an input/output port.  LiteGraph's default is ±15×10 px
  // which extends 15 px into the node body on the input side.  We narrow it to
  // ±8×5 px so the user must click within ~8 px of the port circle itself.
  const originalProcessMouseDown = (LGraphCanvas.prototype as any).processMouseDown;
  (LGraphCanvas.prototype as any).processMouseDown = function (e: any) {
    const node: any = this.graph?.getNodeOnPos?.(e.canvasX, e.canvasY, this.visible_nodes);
    if (!node) return originalProcessMouseDown.call(this, e);

    const TIGHT_H = 8;
    const TIGHT_V = 5;
    const originalGetConnectionPos = node.getConnectionPos.bind(node);
    node.getConnectionPos = function (is_input: boolean, slot: number, out?: Float32Array): [number, number] {
      const pos = originalGetConnectionPos(is_input, slot, out);
      const dx = Math.abs(e.canvasX - pos[0]);
      const dy = Math.abs(e.canvasY - pos[1]);
      if (dx > TIGHT_H || dy > TIGHT_V) {
        // Move the port to a far-away position so the default hit check fails
        if (out) { out[0] = -99999; out[1] = -99999; }
        return [-99999, -99999] as unknown as [number, number];
      }
      return pos;
    };
    try {
      return originalProcessMouseDown.call(this, e);
    } finally {
      node.getConnectionPos = originalGetConnectionPos;
    }
  };
}

function getLinkFanoutInfo(canvas: any, link: any): { index: number; count: number } {
  const links = canvas.graph?.links ? Object.values(canvas.graph.links).filter(Boolean) as any[] : [];
  const siblings = links
    .filter((candidate) => candidate.origin_id === link.origin_id && candidate.origin_slot === link.origin_slot)
    .sort((lhs, rhs) => compareLinkTargets(canvas, lhs, rhs));
  if (siblings.length === 0) return { index: 0, count: 1 };
  const index = siblings.findIndex((candidate) => candidate.id === link.id);
  return { index: index >= 0 ? index : 0, count: siblings.length };
}

function compareLinkTargets(canvas: any, lhs: any, rhs: any): number {
  const left = getLinkTargetSortPoint(canvas, lhs);
  const right = getLinkTargetSortPoint(canvas, rhs);
  return left.y - right.y
    || left.x - right.x
    || (lhs.target_slot ?? 0) - (rhs.target_slot ?? 0)
    || String(lhs.target_id ?? "").localeCompare(String(rhs.target_id ?? ""))
    || (lhs.id ?? 0) - (rhs.id ?? 0);
}

function getLinkTargetSortPoint(canvas: any, link: any): { x: number; y: number } {
  const targetNode = canvas.graph?.getNodeById?.(link.target_id);
  if (targetNode?.getConnectionPos) {
    const pos = new Float32Array(2);
    targetNode.getConnectionPos(true, link.target_slot, pos);
    return { x: pos[0], y: pos[1] };
  }
  return {
    x: targetNode?.pos?.[0] ?? 0,
    y: targetNode?.pos?.[1] ?? 0,
  };
}
