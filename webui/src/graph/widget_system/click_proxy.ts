import { NODE_TITLE_HEIGHT } from "../canvas/rendering";

export function attachWidgetClickProxy(
  lNode: any,
  widget: any,
  onClick: () => void,
): void {
  const previousMouseDown = lNode.onMouseDown;
  lNode.onMouseDown = (e: MouseEvent, pos: [number, number]): boolean | undefined => {
    const margin = 15;
    const widgetHeight = (window as any).LiteGraph?.NODE_WIDGET_HEIGHT ?? 20;
    const widgetTop = typeof widget.last_y === "number"
      ? widget.last_y
      : ((lNode.widgets_start_y ?? (NODE_TITLE_HEIGHT + 8)) as number);
    const widgetBottom = widgetTop + widgetHeight;
    const widgetLeft = margin;
    const widgetRight = (lNode.size?.[0] ?? 0) - margin;

    if (
      pos[0] >= widgetLeft
      && pos[0] <= widgetRight
      && pos[1] >= widgetTop
      && pos[1] <= widgetBottom
    ) {
      e.preventDefault();
      e.stopPropagation();
      onClick();
      return true;
    }

    return previousMouseDown?.(e, pos);
  };
}
