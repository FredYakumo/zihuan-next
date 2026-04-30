export interface CanvasPoint {
  x: number;
  y: number;
}

export interface LinkGeometry {
  start: CanvasPoint;
  control1: CanvasPoint;
  control2: CanvasPoint;
  end: CanvasPoint;
  midPoint: CanvasPoint;
  labelAnchor: CanvasPoint;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function distanceBetween(a: CanvasPoint, b: CanvasPoint): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  return Math.hypot(dx, dy);
}

function normalize(point: CanvasPoint): CanvasPoint {
  const length = Math.hypot(point.x, point.y);
  if (length < 1e-5) return { x: 0, y: -1 };
  return {
    x: point.x / length,
    y: point.y / length,
  };
}

function perpendicular(vector: CanvasPoint): CanvasPoint {
  return { x: -vector.y, y: vector.x };
}

function cubicPoint(
  p0: CanvasPoint,
  p1: CanvasPoint,
  p2: CanvasPoint,
  p3: CanvasPoint,
  t: number,
): CanvasPoint {
  const u = 1 - t;
  const c1 = u * u * u;
  const c2 = 3 * u * u * t;
  const c3 = 3 * u * t * t;
  const c4 = t * t * t;
  return {
    x: c1 * p0.x + c2 * p1.x + c3 * p2.x + c4 * p3.x,
    y: c1 * p0.y + c2 * p1.y + c3 * p2.y + c4 * p3.y,
  };
}

function cubicDerivative(
  p0: CanvasPoint,
  p1: CanvasPoint,
  p2: CanvasPoint,
  p3: CanvasPoint,
  t: number,
): CanvasPoint {
  const u = 1 - t;
  return {
    x: 3 * u * u * (p1.x - p0.x) + 6 * u * t * (p2.x - p1.x) + 3 * t * t * (p3.x - p2.x),
    y: 3 * u * u * (p1.y - p0.y) + 6 * u * t * (p2.y - p1.y) + 3 * t * t * (p3.y - p2.y),
  };
}

function approximateCubicLength(
  p0: CanvasPoint,
  p1: CanvasPoint,
  p2: CanvasPoint,
  p3: CanvasPoint,
  steps = 18,
): number {
  let total = 0;
  let prev = p0;
  for (let i = 1; i <= steps; i++) {
    const point = cubicPoint(p0, p1, p2, p3, i / steps);
    total += distanceBetween(prev, point);
    prev = point;
  }
  return total;
}

export function centeredFanoutIndex(index: number, count: number): number {
  return index - (count - 1) / 2;
}

export function computeLinkGeometry(
  start: CanvasPoint,
  end: CanvasPoint,
  fanoutIndex = 0,
  fanoutCount = 1,
): LinkGeometry {
  const spreadIndex = centeredFanoutIndex(fanoutIndex, fanoutCount);
  const spread = spreadIndex * 22;
  const dx = end.x - start.x;
  const dy = end.y - start.y;
  const absDx = Math.abs(dx);
  const absDy = Math.abs(dy);

  const forwardHandle = clamp(48 + absDx * 0.35 + absDy * 0.08, 54, 180);
  const crossoverBoost = dx < 0
    ? clamp(80 + absDx * 0.55 + absDy * 0.08, 90, 260)
    : 0;
  const handle = clamp(forwardHandle + crossoverBoost, 54, 280);

  const control1: CanvasPoint = {
    x: start.x + handle,
    y: start.y + spread + Math.sign(dy || 1) * Math.min(absDy * 0.08, 26),
  };
  const control2: CanvasPoint = {
    x: end.x - handle,
    y: end.y - spread * 0.55,
  };

  const labelT = fanoutCount > 1 ? 0.27 : 0.33;
  const labelCurvePoint = cubicPoint(start, control1, control2, end, labelT);
  const labelTangent = normalize(cubicDerivative(start, control1, control2, end, labelT));
  let labelNormal = perpendicular(labelTangent);
  if (labelNormal.y > 0) {
    labelNormal = { x: -labelNormal.x, y: -labelNormal.y };
  }
  if (Math.abs(labelNormal.y) < 0.2) {
    labelNormal = { x: labelNormal.x, y: -1 };
  }
  labelNormal = normalize(labelNormal);
  const labelOffset = 12 + Math.min(12, Math.abs(spread) * 0.25);
  const labelAnchor: CanvasPoint = {
    x: labelCurvePoint.x + labelNormal.x * labelOffset,
    y: labelCurvePoint.y + labelNormal.y * labelOffset,
  };

  const baseGeometry: LinkGeometry = {
    start,
    control1,
    control2,
    end,
    midPoint: start,
    labelAnchor,
  };

  return {
    ...baseGeometry,
    midPoint: pointOnLinkGeometry(baseGeometry, 0.5),
  };
}

export function traceLinkPath(ctx: CanvasRenderingContext2D, geometry: LinkGeometry): void {
  ctx.moveTo(geometry.start.x, geometry.start.y);
  ctx.bezierCurveTo(
    geometry.control1.x,
    geometry.control1.y,
    geometry.control2.x,
    geometry.control2.y,
    geometry.end.x,
    geometry.end.y,
  );
}

export function pointOnLinkGeometry(geometry: LinkGeometry, t: number): CanvasPoint {
  const clampedT = clamp(t, 0, 1);
  const curveLength = approximateCubicLength(
    geometry.start,
    geometry.control1,
    geometry.control2,
    geometry.end,
  );
  const local = curveLength <= 1e-5 ? clampedT : clamp((clampedT * curveLength) / curveLength, 0, 1);
  return cubicPoint(
    geometry.start,
    geometry.control1,
    geometry.control2,
    geometry.end,
    local,
  );
}