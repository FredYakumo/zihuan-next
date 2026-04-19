import { LiteGraph } from "litegraph.js";

export function getInlineSlotHeight(): number {
  return (LiteGraph as any).NODE_SLOT_HEIGHT ?? 20;
}

export function getInlineWidgetHeight(): number {
  return (LiteGraph as any).NODE_WIDGET_HEIGHT ?? 20;
}

export function getInlineSlotStartY(node: any): number {
  return (node?.constructor as any)?.slot_start_y ?? 0;
}

export function getInlineRowCenterY(node: any, slotIndex: number): number {
  return getInlineSlotStartY(node) + (slotIndex + 0.7) * getInlineSlotHeight();
}

export function getInlineWidgetTopY(node: any, slotIndex: number): number {
  return getInlineRowCenterY(node, slotIndex) - getInlineWidgetHeight() * 0.5;
}