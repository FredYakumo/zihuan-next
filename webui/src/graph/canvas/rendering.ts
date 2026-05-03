import { LiteGraph } from "litegraph.js";
import type { CanvasFacade } from "./types";
import { getInlineRowCenterY, getInlineWidgetHeight, getInlineWidgetTopY } from "../inline_layout";
import { getBoundaryNodeColors, getLiteGraphColors, onThemeChange } from "../../ui/theme";
import { resolveConcretePortType } from "./type_utils";

export const NODE_TITLE_HEIGHT = 30;
export const DESC_BAND_HEIGHT = 20;

export function bindThemeLifecycle(canvas: CanvasFacade): void {
  applyLiteGraphTheme(canvas);
  onThemeChange(() => applyLiteGraphTheme(canvas));
}

export function applyLiteGraphTheme(canvas: CanvasFacade): void {
  const c = getLiteGraphColors();
  const { lCanvas, lGraph } = canvas;

  (LiteGraph as any).NODE_DEFAULT_COLOR = c.nodeHeader;
  (LiteGraph as any).NODE_DEFAULT_BGCOLOR = c.nodeBg;
  (LiteGraph as any).NODE_DEFAULT_BOXCOLOR = c.nodeBox;
  (LiteGraph as any).NODE_BOX_OUTLINE_COLOR = c.nodeBoxOutline;
  (LiteGraph as any).NODE_TITLE_COLOR = c.nodeTitleText;
  (LiteGraph as any).NODE_SELECTED_TITLE_COLOR = c.nodeSelectedTitle;
  (LiteGraph as any).NODE_TEXT_COLOR = c.nodeText;
  (LiteGraph as any).NODE_TEXT_HIGHLIGHT_COLOR = c.nodeSelectedTitle;
  (LiteGraph as any).DEFAULT_SHADOW_COLOR = c.shadow;
  (LiteGraph as any).WIDGET_BGCOLOR = c.widgetBg;
  (LiteGraph as any).WIDGET_OUTLINE_COLOR = c.widgetOutline;
  (LiteGraph as any).WIDGET_TEXT_COLOR = c.widgetText;
  (LiteGraph as any).WIDGET_SECONDARY_TEXT_COLOR = c.widgetSecondary;
  (LiteGraph as any).WIDGET_DISABLED_TEXT_COLOR = c.widgetDisabled;
  (LiteGraph as any).LINK_COLOR = c.linkColor;
  (LiteGraph as any).EVENT_LINK_COLOR = c.eventLinkColor;
  (LiteGraph as any).CONNECTING_LINK_COLOR = c.connectingLinkColor;

  const tc = c.linkTypeColors;
  const primitives = ["String", "Integer", "Float", "Boolean", "Binary", "Password"];
  const complexTypes = ["Json", "MessageEvent", "OpenAIMessage", "QQMessage", "FunctionTools", "LLModel"];
  const refs = [
    "BotAdapterRef",
    "S3Ref",
    "RedisRef",
    "MySqlRef",
    "TavilyRef",
    "SessionStateRef",
    "OpenAIMessageSessionCacheRef",
    "LoopControlRef",
  ];
  const typeColorMap: Record<string, string> = { Any: tc.any };
  for (const type of primitives) typeColorMap[type] = tc.primitive;
  for (const type of complexTypes) typeColorMap[type] = tc.complex;
  for (const type of refs) typeColorMap[type] = tc.ref;
  for (const type of [...primitives, ...complexTypes, ...refs, "Any"]) {
    typeColorMap[`Vec(${type})`] = tc.array;
    typeColorMap[`Vec<${type}>`] = tc.array;
  }

  const linkTypeColors = (LiteGraph as any).link_type_colors as Record<string, string> | null | undefined;
  if (linkTypeColors) {
    for (const key of Object.keys(linkTypeColors)) delete linkTypeColors[key];
    Object.assign(linkTypeColors, typeColorMap);
  } else {
    (LiteGraph as any).link_type_colors = typeColorMap;
  }

  (lCanvas as any).clear_background_color = c.canvasBg;
  (lCanvas as any).node_title_color = c.nodeTitleText;
  (lCanvas as any).default_link_color = c.linkColor;

  const tile = document.createElement("canvas");
  tile.width = 10;
  tile.height = 10;
  const ctx = tile.getContext("2d")!;
  ctx.fillStyle = c.canvasBg;
  ctx.fillRect(0, 0, 10, 10);
  ctx.fillStyle = c.gridDotColor;
  ctx.beginPath();
  ctx.arc(1, 1, 0.9, 0, Math.PI * 2);
  ctx.fill();
  (lCanvas as any).background_image = tile.toDataURL("image/png");

  const boundaryColors = getBoundaryNodeColors();
  const allNodes: any[] = (lGraph as any)._nodes ?? [];
  for (const node of allNodes) {
    if (node.type === "function_inputs" || node.type === "function_outputs") {
      node.color = boundaryColors.header;
      node.bgcolor = boundaryColors.bg;
    }
  }

  lGraph.setDirtyCanvas(true, true);
}

export function bindCanvasRendering(canvas: CanvasFacade, onNodesMoved: () => void): void {
  const { lCanvas, lGraph } = canvas;

  (LiteGraph as any).alwaysSnapToGrid = false;
  (LiteGraph as any).CANVAS_GRID_SIZE = 10;

  (lCanvas as any).onNodeMoved = (_node: any) => {
    const selected: Record<number, any> | undefined = (lCanvas as any).selected_nodes;
    if (selected && Object.keys(selected).length) {
      for (const node of Object.values(selected)) node.alignToGrid?.();
      lGraph.setDirtyCanvas(true, true);
    }
    canvas.onGraphDirty?.();
    onNodesMoved();
  };

  (lCanvas as any).drawNodeWidgets = bindDrawNodeWidgets(canvas);
  (lCanvas as any).onDrawForeground = (ctx: CanvasRenderingContext2D) => {
    const draggingRect: Float32Array | null = (lCanvas as any).dragging_rectangle ?? null;
    if (draggingRect && draggingRect[2] !== 0) {
      const bgColor: string = (lCanvas as any).clear_background_color || "#222";
      const isLightBg = (parseInt(bgColor.slice(1, 3), 16) || 0) > 128;
      ctx.save();
      ctx.strokeStyle = isLightBg ? "#6030a8" : "#ffffff";
      ctx.lineWidth = 2;
      ctx.setLineDash([5, 5]);
      ctx.fillStyle = isLightBg ? "rgba(96,48,168,0.12)" : "rgba(255,255,255,0.08)";
      ctx.fillRect(draggingRect[0], draggingRect[1], draggingRect[2], draggingRect[3]);
      ctx.strokeRect(draggingRect[0], draggingRect[1], draggingRect[2], draggingRect[3]);
      ctx.setLineDash([]);
      ctx.restore();
    }

    const scale: number = (lCanvas as any).ds?.scale ?? 1;
    if (scale < 0.6) return;
    const links = lGraph.links as Record<number, any>;
    if (!links) return;
    const fontSize = Math.round(10 / scale);
    const titleHeight: number = (LiteGraph as any).NODE_TITLE_HEIGHT ?? 24;
    const allNodes: any[] = (lGraph as any)._nodes ?? [];
    const colors = getLiteGraphColors();
    ctx.save();
    ctx.font = `bold ${fontSize}px sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    const occupiedRects: Array<{ left: number; top: number; right: number; bottom: number }> = [];
    for (const link of Object.values(links)) {
      if (!link || link.origin_id === undefined) continue;
      let typeName: string = String(link.type ?? "");
      if (!typeName || typeName === "*" || typeName === "null" || typeName === "undefined") continue;

      const originNode = lGraph.getNodeById(link.origin_id) as any;
      const targetNode = lGraph.getNodeById(link.target_id) as any;
      const originDef = originNode?.zihuanId && canvas.state.graph
        ? canvas.state.graph.nodes.find((node) => node.id === originNode.zihuanId)
        : undefined;
      const targetDef = targetNode?.zihuanId && canvas.state.graph
        ? canvas.state.graph.nodes.find((node) => node.id === targetNode.zihuanId)
        : undefined;
      if (typeName.includes("Any") && canvas.state.graph && originNode?.zihuanId) {
        const port = originDef?.output_ports[link.origin_slot];
        if (port) {
          const resolved = resolveConcretePortType(canvas.state.graph, originNode.zihuanId, port.name, false);
          if (!resolved.includes("Any")) typeName = resolved;
        }
      }

      const labelPos = getLinkLabelPosition(link);
      if (!labelPos) continue;
      const x = labelPos[0];
      let y = labelPos[1];
      const padding = 3 / scale;
      const labelText = truncateText(ctx, typeName, 240 / scale);
      const metrics = ctx.measureText(labelText);
      const pillWidth = metrics.width + padding * 2;
      const pillHeight = fontSize + padding * 2;
      for (let attempt = 0; attempt < 6; attempt++) {
        let overlapsNode = false;
        for (const node of allNodes) {
          if (!node.pos || !node.size) continue;
          const nx = node.pos[0];
          const ny = node.pos[1] - titleHeight;
          const nw = node.size[0];
          const nh = node.size[1] + titleHeight;
          if (x > nx && x < nx + nw && y > ny && y < ny + nh) {
            y = ny - pillHeight / 2 - 4 / scale;
            overlapsNode = true;
          }
        }

        const rect = {
          left: x - pillWidth / 2,
          top: y - pillHeight / 2,
          right: x + pillWidth / 2,
          bottom: y + pillHeight / 2,
        };
        const overlapsLabel = occupiedRects.some((other) => rectsOverlap(rect, other));
        if (!overlapsNode && !overlapsLabel) {
          occupiedRects.push(rect);
          break;
        }
        y -= pillHeight + 4 / scale;
      }
      const radius = 3 / scale;
      ctx.fillStyle = colors.linkLabelBg;
      ctx.beginPath();
      (ctx as any).roundRect(x - pillWidth / 2, y - pillHeight / 2, pillWidth, pillHeight, radius);
      ctx.fill();
      ctx.fillStyle = colors.linkLabelText;
      ctx.fillText(labelText, x, y);
      void targetDef;
    }
    ctx.restore();
  };
}

function bindDrawNodeWidgets(canvas: CanvasFacade) {
  const origDrawNodeWidgets = (canvas.lCanvas as any).drawNodeWidgets.bind(canvas.lCanvas);
  return (node: any, posY: any, ctx: CanvasRenderingContext2D) => {
    if (node.inputs && node.widgets) {
      for (const input of node.inputs as any[]) {
        if (!input.widget) continue;
        const widget = getInputLinkedWidget(node, input);
        if (widget) widget.disabled = isInlineInputOccupied(node, input) || input.link != null;
      }
    }

    const savedPasswordValues: Array<{ w: any; real: any }> = [];
    if (node.widgets) {
      for (const widget of node.widgets as any[]) {
        if (widget._isPassword) {
          savedPasswordValues.push({ w: widget, real: widget.value });
          widget.value = "•".repeat(String(widget.value ?? "").length || 8);
        }
      }
    }
    origDrawNodeWidgets(node, posY, ctx);
    for (const { w, real } of savedPasswordValues) w.value = real;

    if (node.widgets) {
      const c = getLiteGraphColors();
      const widgetHeight: number = (LiteGraph as any).NODE_WIDGET_HEIGHT ?? 20;
      const margin = 15;
      const nodeWidth: number = node.size[0];
      const showText: boolean = (canvas.lCanvas as any).ds?.scale > 0.5;
      const isInline = !!node._hasInlineWidgets;
      ctx.save();
      ctx.globalAlpha = (canvas.lCanvas as any).editor_alpha ?? 1;
      for (const widget of node.widgets as any[]) {
        if (widget.last_y === undefined) continue;
        const widgetWidth: number = widget.width || nodeWidth;
        const inlineInputIdx = isInline ? getInlineWidgetInputIndex(node, widget) : -1;
        const inlineInput = inlineInputIdx >= 0 ? (node.inputs as any[])?.[inlineInputIdx] : undefined;
        const inlineRowOccupied = !!inlineInput && isInlineInputOccupied(node, inlineInput);
        const inlineRowCenterY = inlineInputIdx >= 0
          ? getInlineRowCenterY(node, inlineInputIdx)
          : widget.last_y + widgetHeight * 0.5;

        if (isInline) {
          ctx.fillStyle = c.nodeBg;
          ctx.fillRect(margin, widget.last_y, widgetWidth - margin * 2, widgetHeight);
          if (inlineRowOccupied) continue;

          if (widget.type === "button") {
            const inlineRight = 25;
            const valueBoxWidth = 70;
            const boxRightX = widgetWidth - margin - inlineRight;
            const boxLeftX = Math.max(margin + 5, boxRightX - valueBoxWidth);
            const boxWidth = boxRightX - boxLeftX;
            ctx.fillStyle = c.widgetButtonBg;
            ctx.strokeStyle = (LiteGraph as any).WIDGET_OUTLINE_COLOR;
            ctx.fillRect(boxLeftX, widget.last_y, boxWidth, widgetHeight);
            if (showText && !widget.disabled) ctx.strokeRect(boxLeftX, widget.last_y, boxWidth, widgetHeight);
            if (showText) {
              ctx.fillStyle = c.widgetButtonText;
              ctx.textAlign = "center";
              ctx.textBaseline = "middle";
              ctx.font = `${(LiteGraph as any).NODE_TEXT_SIZE ?? 14}px Arial`;
              ctx.fillText(widget.label || widget.name, boxLeftX + boxWidth * 0.5, inlineRowCenterY);
            }
          } else if ((widget.type === "text" || widget.type === "number" || widget.type === "combo") && showText) {
            const rowOutput = inlineInputIdx >= 0 ? (node.outputs as any[])?.[inlineInputIdx] : undefined;
            const inputAtRow = inlineInputIdx >= 0 ? (node.inputs as any[])?.[inlineInputIdx] : undefined;
            const isSameNamePassthrough = rowOutput && rowOutput.name === inputAtRow?.name;
            let valueRightX = widgetWidth - margin;
            if (rowOutput && !isSameNamePassthrough) {
              const tempFontSize: number = (LiteGraph as any).NODE_SUBTEXT_SIZE ?? 12;
              const slotHeight: number = (LiteGraph as any).NODE_SLOT_HEIGHT ?? 20;
              ctx.font = `${tempFontSize}px Arial`;
              const outLabel: string = rowOutput.label ?? rowOutput.name ?? "";
              const outLabelWidth = outLabel ? ctx.measureText(outLabel).width : 0;
              const outLabelRightX = nodeWidth - slotHeight - 2;
              valueRightX = Math.min(valueRightX, outLabelRightX - outLabelWidth - 4);
            }
            const inputLabel: string = inputAtRow?.name ?? "";
            const slotHeight = (LiteGraph as any).NODE_SLOT_HEIGHT ?? 20;
            const labelTextX = slotHeight + 2;
            const labelFontSize = (LiteGraph as any).NODE_SUBTEXT_SIZE ?? 12;
            ctx.font = `${labelFontSize}px Arial`;
            const inputLabelWidth = inputLabel ? ctx.measureText(inputLabel).width : 0;
            const valueLeftX = labelTextX + inputLabelWidth + 6;

            if (valueRightX > valueLeftX + 10) {
              let value: string;
              if (widget.type === "number") {
                value = Number(widget.value).toFixed(widget.options?.precision ?? 3);
              } else if (widget.type === "combo") {
                let comboValue = widget.value;
                const values = widget.options?.values;
                if (values && values.constructor !== Array) {
                  comboValue = values[widget.value] ?? widget.value;
                }
                value = String(comboValue ?? "");
              } else {
                value = String(widget.value ?? "");
              }
              const fontSize = (LiteGraph as any).NODE_SUBTEXT_SIZE ?? 12;
              ctx.font = `${fontSize}px Arial`;
              const truncatedValue = truncateText(ctx, value, Math.max(0, valueRightX - valueLeftX));
              ctx.textAlign = "left";
              ctx.textBaseline = "middle";
              ctx.fillStyle = widget.disabled ? c.widgetDisabled : c.widgetText;
              ctx.fillText(truncatedValue, valueLeftX, inlineRowCenterY);
              if (widget.type === "combo" && !widget.disabled) {
                ctx.fillStyle = widget.disabled ? c.widgetDisabled : c.widgetText;
                ctx.textAlign = "right";
                ctx.fillText("▾", valueRightX, inlineRowCenterY);
              }
            }
          }
        } else {
          const contentWidth = widgetWidth - margin * 2;
          if (widget.type === "button") {
            ctx.fillStyle = c.widgetButtonBg;
            ctx.strokeStyle = (LiteGraph as any).WIDGET_OUTLINE_COLOR;
            ctx.fillRect(margin, widget.last_y, contentWidth, widgetHeight);
            if (showText && !widget.disabled) ctx.strokeRect(margin, widget.last_y, contentWidth, widgetHeight);
            if (showText) {
              ctx.fillStyle = c.widgetButtonText;
              ctx.textAlign = "center";
              ctx.font = `${(LiteGraph as any).NODE_TEXT_SIZE ?? 14}px Arial`;
              ctx.fillText(widget.label || widget.name, widgetWidth * 0.5, widget.last_y + widgetHeight * 0.7);
            }
          } else if ((widget.type === "text" || widget.type === "number") && showText) {
            const label = widget.label || widget.name || "";
            const labelWidth = label ? ctx.measureText(label).width + 8 : 0;
            const maxValueWidth = Math.max(0, contentWidth - labelWidth - 20);
            const value = widget.type === "number"
              ? Number(widget.value).toFixed(widget.options?.precision ?? 3)
              : String(widget.value ?? "");
            const truncatedValue = truncateText(ctx, value, maxValueWidth);
            ctx.font = `${(LiteGraph as any).NODE_TEXT_SIZE ?? 14}px Arial`;
            ctx.textAlign = "right";
            ctx.fillStyle = widget.disabled ? c.widgetDisabled : c.widgetText;
            ctx.fillText(truncatedValue, widgetWidth - margin * 2, widget.last_y + widgetHeight * 0.7);
          }
        }
      }
      ctx.restore();
    }

    drawWidgetBindingBadges.call(node, ctx);
    drawInlineInputLabels(node, ctx);
    drawInlineOutputLabels(node, ctx);
  };
}

export function getInlineWidgetInputIndex(node: any, widget: any): number {
  if (typeof widget?._inlineInputIndex === "number") return widget._inlineInputIndex;
  return (node.inputs as any[])?.findIndex(
    (input: any) => input.widget && (typeof input.widget === "object" ? input.widget.name : input.widget) === widget?.name,
  ) ?? -1;
}

export function getInputLinkedWidget(node: any, input: any): any | null {
  if (!node?.widgets || !input?.widget) return null;
  const widgetName: string = typeof input.widget === "object" ? input.widget.name : input.widget;
  return (node.widgets as any[]).find((widget: any) => widget?.name === widgetName) ?? null;
}

export function isInlineInputOccupied(node: any, input: any): boolean {
  if (!node?._hasInlineWidgets || !input?.widget) return false;
  return input.link != null || !!node._portBindings?.[input.name];
}

export function findInlineInputAtPosition(
  node: any,
  graphX: number,
  graphY: number,
): { slot: number; input: any } | null {
  if (!node?._hasInlineWidgets || !node?.inputs?.length || !node?.pos || !node?.size) return null;

  const localX = graphX - node.pos[0];
  const localY = graphY - node.pos[1];
  const widgetWidth = Number(node.size[0] ?? 0);
  if (widgetWidth <= 0) return null;
  if (localX < 6 || localX > widgetWidth - 12) return null;

  const widgetHeight = getInlineWidgetHeight();
  for (let i = 0; i < node.inputs.length; i++) {
    const input = node.inputs[i];
    if (!input?.widget) continue;
    const top = getInlineWidgetTopY(node, i);
    if (localY >= top && localY <= top + widgetHeight) {
      return { slot: i, input };
    }
  }
  return null;
}

export function drawHelpButton(this: any, ctx: CanvasRenderingContext2D): void {
  if (!this._helpVisible) return;
  const cx = (this.size[0] as number) - 14;
  const cy = -NODE_TITLE_HEIGHT / 2;
  const r = 8;
  ctx.save();
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.fillStyle = "rgba(255,255,255,0.18)";
  ctx.fill();
  ctx.strokeStyle = "rgba(255,255,255,0.55)";
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.font = "bold 11px sans-serif";
  ctx.fillStyle = "#ffffff";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("?", cx, cy + 0.5);
  ctx.restore();
}

export function drawBindingBadges(this: any, ctx: CanvasRenderingContext2D): void {
  const bindings: Record<string, { kind: string; name: string }> = this._portBindings;
  if (!bindings || !this.inputs) return;

  const slotHeight = 20;
  const fontSize = 12;
  const labelX = slotHeight * 0.5 + 10;
  const baselineOffset = 5;

  ctx.save();
  ctx.font = `normal ${fontSize}px Arial`;
  ctx.textBaseline = "middle";

  let verticalSlotIndex = -1;
  for (let i = 0; i < this.inputs.length; i++) {
    const input = this.inputs[i];
    if (input.pos || (this.widgets?.length && input.widget)) continue;
    verticalSlotIndex++;
    const binding = bindings[input.name];
    if (!binding) continue;
    const localY = (verticalSlotIndex + 0.7) * slotHeight + ((this.constructor as any).slot_start_y || 0);
    drawBadgePill(ctx, input.name, binding, labelX, localY + baselineOffset, fontSize, this.size[0]);
  }
  ctx.restore();
}

function drawWidgetBindingBadges(this: any, ctx: CanvasRenderingContext2D): void {
  const bindings: Record<string, { kind: string; name: string }> = this._portBindings;
  if (!bindings || !this.inputs || !this.widgets) return;
  const fontSize = 12;
  const labelX = 30;
  ctx.save();
  ctx.font = `normal ${fontSize}px Arial`;
  ctx.textBaseline = "middle";
  for (let i = 0; i < this.inputs.length; i++) {
    const input = this.inputs[i];
    if (!input.widget) continue;
    const binding = bindings[input.name];
    if (!binding) continue;
    drawBadgePill(ctx, input.name, binding, labelX, getInlineRowCenterY(this, i), fontSize, this.size[0]);
  }
  ctx.restore();
}

function drawBadgePill(
  ctx: CanvasRenderingContext2D,
  portName: string,
  binding: { kind: string; name: string },
  labelX: number,
  centerY: number,
  fontSize: number,
  nodeWidth: number,
): void {
  const labelWidth = ctx.measureText(portName).width;
  const badgePrefix = binding.kind === "hyperparameter" ? "\u2191" : "\u27f2";
  const fullBadgeText = badgePrefix + binding.name;
  const badgePadX = 4;
  const badgePadY = 2;
  const badgeH = fontSize + badgePadY * 2;
  const badgeX = labelX + labelWidth + 4;
  const badgeY = centerY - badgeH / 2;
  const maxBadgeWidth = Math.max(0, nodeWidth - badgeX - 10);
  const prefixWidth = ctx.measureText(badgePrefix).width;
  let badgeText = fullBadgeText;
  if (maxBadgeWidth <= 0) return;
  if (ctx.measureText(fullBadgeText).width > maxBadgeWidth) {
    if (prefixWidth + ctx.measureText("…").width <= maxBadgeWidth) {
      badgeText = truncateText(ctx, fullBadgeText, maxBadgeWidth);
    } else if (prefixWidth <= maxBadgeWidth) {
      badgeText = badgePrefix;
    } else {
      return;
    }
  }
  const badgeTextW = ctx.measureText(badgeText).width;
  const badgeW = badgeTextW + badgePadX * 2;
  const bgColor = binding.kind === "hyperparameter" ? "#e67e22" : "#1abc9c";
  ctx.fillStyle = bgColor;
  ctx.beginPath();
  (ctx as any).roundRect(badgeX, badgeY, badgeW, badgeH, 3);
  ctx.fill();
  ctx.fillStyle = "#ffffff";
  ctx.fillText(badgeText, badgeX + badgePadX, badgeY + badgeH / 2);
}

function drawInlineInputLabels(node: any, ctx: CanvasRenderingContext2D): void {
  if (!node._hasInlineWidgets || !node.inputs?.length) return;
  const fontSize: number = (LiteGraph as any).NODE_SUBTEXT_SIZE ?? 12;
  const textColor = getLiteGraphColors().nodeText;
  const slotHeight: number = (LiteGraph as any).NODE_SLOT_HEIGHT ?? 20;
  const textX = slotHeight + 2;
  ctx.save();
  ctx.font = `${fontSize}px Arial`;
  ctx.textAlign = "left";
  ctx.textBaseline = "middle";
  for (let i = 0; i < node.inputs.length; i++) {
    const input = node.inputs[i];
    if (!input?.widget) continue;
    if (!input.name) continue;
    ctx.fillStyle = textColor;
    ctx.fillText(input.name, textX, getInlineRowCenterY(node, i));
  }
  ctx.restore();
}

function drawInlineOutputLabels(node: any, ctx: CanvasRenderingContext2D): void {
  if (!node._hasInlineWidgets || !node.outputs?.length) return;
  const fontSize: number = (LiteGraph as any).NODE_SUBTEXT_SIZE ?? 12;
  const colors = getLiteGraphColors();
  const nodeWidth: number = node.size[0];
  const slotHeight: number = (LiteGraph as any).NODE_SLOT_HEIGHT ?? 20;
  const textX = nodeWidth - slotHeight - 2;
  ctx.save();
  ctx.font = `${fontSize}px Arial`;
  ctx.textAlign = "right";
  ctx.textBaseline = "middle";
  for (let i = 0; i < node.outputs.length; i++) {
    const output = node.outputs[i];
    if (!output) continue;
    const label = output.label != null ? String(output.label) : output.name;
    if (!label) continue;
    const correspondingInput = (node.inputs as any[])?.[i];
    if (correspondingInput?.widget && correspondingInput.name === output.name) continue;
    const localY = getInlineRowCenterY(node, i);
    const textMetrics = ctx.measureText(label);
    const bgW = textMetrics.width + 8;
    const bgH = fontSize + 4;
    ctx.fillStyle = node.bgcolor || colors.nodeBg;
    ctx.fillRect(textX - textMetrics.width - 4, localY - bgH / 2, bgW, bgH);
    ctx.fillStyle = colors.nodeText;
    ctx.fillText(label, textX, localY);
  }
  ctx.restore();
}

function getLinkLabelPosition(link: any): [number, number] | null {
  const customPos = link?._zhLabelPos as ArrayLike<number> | undefined;
  if (customPos && typeof customPos.length === "number" && customPos.length >= 2) {
    return [customPos[0], customPos[1]];
  }
  const pos = link?._pos as ArrayLike<number> | null | undefined;
  if (pos && typeof pos.length === "number" && pos.length >= 2) {
    return [pos[0], pos[1]];
  }
  return null;
}

function rectsOverlap(
  a: { left: number; top: number; right: number; bottom: number },
  b: { left: number; top: number; right: number; bottom: number },
): boolean {
  return !(a.right < b.left || a.left > b.right || a.bottom < b.top || a.top > b.bottom);
}

export function findRegisteredType(typeId: string): string | null {
  const nodeTypes = (LiteGraph as any).registered_node_types as Record<string, unknown>;
  for (const key of Object.keys(nodeTypes)) {
    const cls = nodeTypes[key] as any;
    if (cls.zihuanTypeId === typeId) return key;
  }
  return null;
}

export function truncateText(ctx: CanvasRenderingContext2D, text: string, maxWidth: number): string {
  if (maxWidth <= 0) return "";
  if (ctx.measureText(text).width <= maxWidth) return text;
  const ellipsis = "…";
  const ellipsisW = ctx.measureText(ellipsis).width;
  if (maxWidth <= ellipsisW) return ellipsis;
  let lo = 0;
  let hi = text.length;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    const width = ctx.measureText(text.slice(0, mid) + ellipsis).width;
    if (width <= maxWidth) lo = mid;
    else hi = mid - 1;
  }
  return text.slice(0, lo) + ellipsis;
}
