// Ellipses and splines: where they run, for drawing, hit testing and export.
// An ellipse is its centre, the end of its major semi-axis and its minor
// semi-axis; it runs counter-clockwise, so point(t) = c + u·cos t + v·sin t
// with v the major axis turned a quarter left and scaled to the minor one.
// A spline is a clamped (rational) B-spline evaluated by de Boor.
import { point, type Ellipse, type EllipseArc, type Sketch, type Spline } from './model';

export interface Vec { x: number; y: number }

/** An ellipse's centre and semi-axes as vectors. */
export interface EllipseFrame { c: Vec; u: Vec; v: Vec }

export function ellipseFrame(sketch: Sketch, e: Ellipse | EllipseArc): EllipseFrame {
  const c = point(sketch, e.c), m = point(sketch, e.m);
  const u = { x: m.x - c.x, y: m.y - c.y };
  const a = Math.hypot(u.x, u.y) || 1;
  return { c, u, v: { x: (-u.y / a) * e.r, y: (u.x / a) * e.r } };
}

export const onEllipse = (f: EllipseFrame, t: number): Vec =>
  ({ x: f.c.x + f.u.x * Math.cos(t) + f.v.x * Math.sin(t), y: f.c.y + f.u.y * Math.cos(t) + f.v.y * Math.sin(t) });

/** The parameter of the point of the ellipse nearest to the ray through `p`. */
export function ellipseParam(f: EllipseFrame, p: Vec): number {
  const d = { x: p.x - f.c.x, y: p.y - f.c.y };
  const uu = f.u.x * f.u.x + f.u.y * f.u.y || 1, vv = f.v.x * f.v.x + f.v.y * f.v.y || 1;
  return Math.atan2((d.x * f.v.x + d.y * f.v.y) / vv, (d.x * f.u.x + d.y * f.u.y) / uu);
}

/** An elliptical arc's start and end parameters, the end above the start. */
export function arcParams(sketch: Sketch, e: EllipseArc): [number, number] {
  const f = ellipseFrame(sketch, e);
  const start = ellipseParam(f, point(sketch, e.a));
  let end = ellipseParam(f, point(sketch, e.b));
  while (end <= start + 1e-9) end += 2 * Math.PI;
  return [start, end];
}

/** The point of a spline at parameter `u`, within its knots. */
export function splineAt(points: readonly Vec[], degree: number, knots: readonly number[], weights: readonly number[] | undefined, u: number): Vec {
  const n = points.length, p = Math.min(degree, n - 1);
  const top = knots[n] ?? 1;
  let span = p;
  if (u >= top) span = n - 1;
  else while (span < n - 1 && knots[span + 1]! <= u) span++;
  const d: [number, number, number][] = [];
  for (let j = 0; j <= p; j++) {
    const i = span - p + j, w = weights?.[i] ?? 1, c = points[i]!;
    d.push([c.x * w, c.y * w, w]);
  }
  for (let r = 1; r <= p; r++) {
    for (let j = p; j >= r; j--) {
      const i = span - p + j;
      const den = knots[i + p + 1 - r]! - knots[i]!;
      const alpha = den === 0 ? 0 : (u - knots[i]!) / den;
      const prev = d[j - 1]!, cur = d[j]!;
      d[j] = [prev[0] * (1 - alpha) + cur[0] * alpha, prev[1] * (1 - alpha) + cur[1] * alpha, prev[2] * (1 - alpha) + cur[2] * alpha];
    }
  }
  const last = d[p]!;
  return { x: last[0] / last[2], y: last[1] / last[2] };
}

/** A spline's parameter range. */
export const splineDomain = (e: Spline): [number, number] => [e.knots[e.degree] ?? 0, e.knots[e.poles.length] ?? 1];

/** The curve as a polyline fine enough to draw and to hit: an ellipse in
 *  96 steps a turn, a spline in 24 per span. */
export function sampled(sketch: Sketch, e: Ellipse | EllipseArc | Spline): Vec[] {
  if (e.kind === 'spline') {
    const poles = e.poles.map((id) => point(sketch, id));
    const [from, to] = splineDomain(e);
    const spans = Math.max(1, new Set(e.knots.slice(e.degree, e.poles.length + 1)).size - 1);
    const steps = Math.min(4000, spans * 24 * Math.max(1, e.degree));
    return Array.from({ length: steps + 1 }, (_, i) => splineAt(poles, e.degree, e.knots, e.weights, from + ((to - from) * i) / steps));
  }
  const f = ellipseFrame(sketch, e);
  const [start, end] = e.kind === 'ellipse' ? [0, 2 * Math.PI] : arcParams(sketch, e);
  const steps = Math.max(8, Math.ceil(((end - start) / (2 * Math.PI)) * 96));
  return Array.from({ length: steps + 1 }, (_, i) => onEllipse(f, start + ((end - start) * i) / steps));
}
