// Plane geometry for the sketch: angles in radians, counter-clockwise from +X.
import { point, type Entity, type Sketch } from './model';
import { sampled } from './curves';

export interface Vec { x: number; y: number }

const TAU = Math.PI * 2;
/** Below this, in millimetres, two positions are the same. */
export const EPS = 1e-9;

export const sub = (a: Vec, b: Vec): Vec => ({ x: a.x - b.x, y: a.y - b.y });
export const add = (a: Vec, b: Vec): Vec => ({ x: a.x + b.x, y: a.y + b.y });
export const scale = (a: Vec, k: number): Vec => ({ x: a.x * k, y: a.y * k });
export const dot = (a: Vec, b: Vec): number => a.x * b.x + a.y * b.y;
export const cross = (a: Vec, b: Vec): number => a.x * b.y - a.y * b.x;
export const len = (a: Vec): number => Math.hypot(a.x, a.y);
export const dist = (a: Vec, b: Vec): number => Math.hypot(a.x - b.x, a.y - b.y);
export const mid = (a: Vec, b: Vec): Vec => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
export const polar = (c: Vec, r: number, angle: number): Vec => ({ x: c.x + r * Math.cos(angle), y: c.y + r * Math.sin(angle) });
export const angleOf = (from: Vec, to: Vec): number => Math.atan2(to.y - from.y, to.x - from.x);

/** An angle in [0, 2π). */
export function normalize(angle: number): number {
  const a = angle % TAU;
  return a < 0 ? a + TAU : a;
}

/** The counter-clockwise sweep from `start` to `end`, in (0, 2π]. */
export function sweep(start: number, end: number): number {
  const s = normalize(end - start);
  return s < EPS ? TAU : s;
}

/** Whether `angle` lies on the counter-clockwise arc from `start` to `end`. */
export function onSweep(angle: number, start: number, end: number): boolean {
  return normalize(angle - start) <= sweep(start, end) + 1e-12;
}

/** The circle through three points, or null when they are in line. */
export function circleThrough(a: Vec, b: Vec, c: Vec): { c: Vec; r: number } | null {
  const d = 2 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
  if (Math.abs(d) < 1e-12) return null;
  const a2 = a.x * a.x + a.y * a.y, b2 = b.x * b.x + b.y * b.y, c2 = c.x * c.x + c.y * c.y;
  const center = {
    x: (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
    y: (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
  };
  return { c: center, r: dist(center, a) };
}

/** A resolved curve, with its points looked up. */
export type Shape =
  | { kind: 'line'; a: Vec; b: Vec }
  | { kind: 'circle'; c: Vec; r: number }
  | { kind: 'arc'; c: Vec; r: number; start: number; end: number }
  /** An ellipse or spline, as the polyline it is drawn and hit with. */
  | { kind: 'poly'; pts: Vec[]; closed: boolean };

/** The line pieces of a polyline. */
const pieces = (s: Extract<Shape, { kind: 'poly' }>): Extract<Shape, { kind: 'line' }>[] =>
  s.pts.slice(1).map((b, i) => ({ kind: 'line', a: s.pts[i]!, b }));

export function shapeOf(sketch: Sketch, e: Entity): Shape {
  if (e.kind === 'line') return { kind: 'line', a: point(sketch, e.a), b: point(sketch, e.b) };
  if (e.kind === 'circle') return { kind: 'circle', c: point(sketch, e.c), r: e.r };
  if (e.kind !== 'arc') return { kind: 'poly', pts: sampled(sketch, e), closed: e.kind === 'ellipse' };
  const c = point(sketch, e.c), a = point(sketch, e.a), b = point(sketch, e.b);
  return { kind: 'arc', c, r: dist(c, a), start: angleOf(c, a), end: angleOf(c, b) };
}

/** The closest point of a shape to `p`. */
export function nearest(shape: Shape, p: Vec): Vec {
  if (shape.kind === 'poly') {
    let best = shape.pts[0] ?? p, d = Infinity;
    for (const piece of pieces(shape)) { const q = nearest(piece, p), dq = dist(p, q); if (dq < d) { d = dq; best = q; } }
    return best;
  }
  if (shape.kind === 'line') {
    const d = sub(shape.b, shape.a);
    const l2 = dot(d, d);
    const t = l2 < EPS ? 0 : Math.max(0, Math.min(1, dot(sub(p, shape.a), d) / l2));
    return add(shape.a, scale(d, t));
  }
  const angle = angleOf(shape.c, p);
  if (shape.kind === 'circle' || onSweep(angle, shape.start, shape.end)) return polar(shape.c, shape.r, angle);
  const s = polar(shape.c, shape.r, shape.start), e = polar(shape.c, shape.r, shape.end);
  return dist(p, s) <= dist(p, e) ? s : e;
}

export const distanceTo = (shape: Shape, p: Vec): number => dist(p, nearest(shape, p));

/** The midpoint along a line or arc; a circle has none. */
export function midOf(shape: Shape): Vec | null {
  if (shape.kind === 'line') return mid(shape.a, shape.b);
  if (shape.kind === 'arc') return polar(shape.c, shape.r, shape.start + sweep(shape.start, shape.end) / 2);
  if (shape.kind === 'poly' && !shape.closed) return shape.pts[Math.floor(shape.pts.length / 2)] ?? null;
  return null;
}

/** Whether a point found on a shape's full line or circle also lies on the shape itself. */
function within(shape: Shape, p: Vec): boolean {
  if (shape.kind === 'line') {
    const d = sub(shape.b, shape.a);
    const l2 = dot(d, d);
    if (l2 < EPS) return false;
    const t = dot(sub(p, shape.a), d) / l2;
    return t >= -1e-9 && t <= 1 + 1e-9;
  }
  if (shape.kind === 'poly') return true;
  return shape.kind === 'circle' || onSweep(angleOf(shape.c, p), shape.start, shape.end);
}

/** Where two shapes cross. */
export function intersections(s: Shape, t: Shape): Vec[] {
  if (s.kind === 'poly') return pieces(s).flatMap((piece) => intersections(piece, t));
  if (t.kind === 'poly') return pieces(t).flatMap((piece) => intersections(s, piece));
  let found: Vec[];
  if (s.kind === 'line' && t.kind === 'line') found = lineLine(s.a, s.b, t.a, t.b);
  else if (s.kind === 'line') found = lineCircle(s.a, s.b, (t as Extract<Shape, { kind: 'circle' | 'arc' }>).c, (t as Extract<Shape, { kind: 'circle' | 'arc' }>).r);
  else if (t.kind === 'line') found = lineCircle(t.a, t.b, s.c, s.r);
  else found = circleCircle(s.c, s.r, t.c, t.r);
  return found.filter((p) => within(s, p) && within(t, p));
}

function lineLine(a: Vec, b: Vec, c: Vec, d: Vec): Vec[] {
  const r = sub(b, a), s = sub(d, c);
  const denom = cross(r, s);
  if (Math.abs(denom) < 1e-12) return [];
  const t = cross(sub(c, a), s) / denom;
  return [add(a, scale(r, t))];
}

function lineCircle(a: Vec, b: Vec, c: Vec, r: number): Vec[] {
  const d = sub(b, a);
  const f = sub(a, c);
  const A = dot(d, d);
  if (A < EPS) return [];
  const B = 2 * dot(f, d);
  const C = dot(f, f) - r * r;
  const disc = B * B - 4 * A * C;
  if (disc < -1e-9) return [];
  if (disc <= 1e-9) return [add(a, scale(d, -B / (2 * A)))];
  const root = Math.sqrt(disc);
  return [add(a, scale(d, (-B - root) / (2 * A))), add(a, scale(d, (-B + root) / (2 * A)))];
}

function circleCircle(c1: Vec, r1: number, c2: Vec, r2: number): Vec[] {
  const d = dist(c1, c2);
  if (d < EPS || d > r1 + r2 + 1e-9 || d < Math.abs(r1 - r2) - 1e-9) return [];
  const a = (r1 * r1 - r2 * r2 + d * d) / (2 * d);
  const h = Math.sqrt(Math.max(0, r1 * r1 - a * a));
  const m = add(c1, scale(sub(c2, c1), a / d));
  if (h < 1e-9) return [m];
  const off = { x: (-(c2.y - c1.y) * h) / d, y: ((c2.x - c1.x) * h) / d };
  return [add(m, off), sub(m, off)];
}

export interface Bounds { minX: number; minY: number; maxX: number; maxY: number }

/** The extent of a shape, arcs bounded by their quadrant points. */
export function boundsOf(shape: Shape): Bounds {
  if (shape.kind === 'poly') {
    const xs = shape.pts.map((p) => p.x), ys = shape.pts.map((p) => p.y);
    return { minX: Math.min(...xs), minY: Math.min(...ys), maxX: Math.max(...xs), maxY: Math.max(...ys) };
  }
  if (shape.kind === 'line') {
    return { minX: Math.min(shape.a.x, shape.b.x), minY: Math.min(shape.a.y, shape.b.y), maxX: Math.max(shape.a.x, shape.b.x), maxY: Math.max(shape.a.y, shape.b.y) };
  }
  if (shape.kind === 'circle') return { minX: shape.c.x - shape.r, minY: shape.c.y - shape.r, maxX: shape.c.x + shape.r, maxY: shape.c.y + shape.r };
  const pts = [polar(shape.c, shape.r, shape.start), polar(shape.c, shape.r, shape.end)];
  for (let q = 0; q < 4; q++) if (onSweep((q * Math.PI) / 2, shape.start, shape.end)) pts.push(polar(shape.c, shape.r, (q * Math.PI) / 2));
  return {
    minX: Math.min(...pts.map((p) => p.x)), minY: Math.min(...pts.map((p) => p.y)),
    maxX: Math.max(...pts.map((p) => p.x)), maxY: Math.max(...pts.map((p) => p.y)),
  };
}

export function unionBounds(all: Bounds[]): Bounds | null {
  if (!all.length) return null;
  return {
    minX: Math.min(...all.map((b) => b.minX)), minY: Math.min(...all.map((b) => b.minY)),
    maxX: Math.max(...all.map((b) => b.maxX)), maxY: Math.max(...all.map((b) => b.maxY)),
  };
}

export const sketchBounds = (sketch: Sketch): Bounds | null =>
  unionBounds([
    ...sketch.entities.map((e) => boundsOf(shapeOf(sketch, e))),
    ...(sketch.texts ?? []).map((t) => ({ minX: t.x, minY: t.y, maxX: t.x + t.width, maxY: t.y + t.height })),
  ]);

/** Whether a shape lies entirely inside a box. */
export function insideBox(shape: Shape, box: Bounds): boolean {
  const b = boundsOf(shape);
  return b.minX >= box.minX && b.maxX <= box.maxX && b.minY >= box.minY && b.maxY <= box.maxY;
}

/** Whether any part of a shape lies inside a box. */
export function touchesBox(shape: Shape, box: Bounds): boolean {
  if (insideBox(shape, box)) return true;
  const corners = [{ x: box.minX, y: box.minY }, { x: box.maxX, y: box.minY }, { x: box.maxX, y: box.maxY }, { x: box.minX, y: box.maxY }];
  for (let i = 0; i < 4; i++) {
    const edge: Shape = { kind: 'line', a: corners[i]!, b: corners[(i + 1) % 4]! };
    if (intersections(shape, edge).length) return true;
  }
  // A shape with an end inside the box but crossing nothing is already inside.
  const probe = shape.kind === 'line' ? shape.a : shape.kind === 'poly' ? shape.pts[0]! : polar(shape.c, shape.r, shape.kind === 'arc' ? shape.start : 0);
  return probe.x >= box.minX && probe.x <= box.maxX && probe.y >= box.minY && probe.y <= box.maxY;
}
