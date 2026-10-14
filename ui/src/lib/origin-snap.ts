// Where a tap on the drawing puts the origin: the nearest of the points
// a drawing offers (corners, ends, edge middles, arc and circle centres,
// shape and part centres, crossings), else the tapped point itself.
import type { PreviewContour } from '../api';

type XY = [number, number];
type Bounds = { min: { x: number; y: number }; max: { x: number; y: number } };

/** What a snapped point is, for the hint beside it. */
export type SnapKind = 'corner' | 'end' | 'middle' | 'centre' | 'shape' | 'part' | 'crossing';

/** A point the origin can snap to. */
export type Snap = { at: XY; kind: SnapKind };

/** Edges past this many are not crossed with each other: a picture's
 *  thousands of tiny steps would take too long and offer nothing useful. */
const CROSSING_EDGES = 1500;

/** Every point the drawn contours offer, as cut, and the part's nine. */
export function originCandidates(contours: PreviewContour[], bounds: Bounds | null = null): Snap[] {
  const out: Snap[] = [];
  const edges: [XY, XY][] = [];
  for (const contour of contours) {
    const points = contour.paths.filter((p) => p.kind === 'cut' || p.kind === 'film').flatMap((p) => p.points);
    // A closed line ends where it began: once is enough.
    const [first, last] = [points[0], points[points.length - 1]];
    if (contour.closed && first && last && points.length > 2 && dist(first, last) < 1e-6) points.pop();
    if (points.length < 2) continue;
    const circle = circleCentre(points, contour.closed);
    if (circle) { out.push({ at: circle, kind: 'centre' }); continue; }
    const turns = cornerIndices(points, contour.closed);
    for (const i of turns) out.push({ at: points[i]!, kind: 'corner' });
    if (!contour.closed) out.push({ at: points[0]!, kind: 'end' }, { at: points[points.length - 1]!, kind: 'end' });
    for (const run of runs(points, turns, contour.closed)) {
      if (run.length < 2) continue;
      const a = run[0]!, b = run[run.length - 1]!;
      const arc = run.length >= 3 ? arcCentre(run) : null;
      if (arc) {
        out.push({ at: arc, kind: 'centre' }, { at: run[Math.floor(run.length / 2)]!, kind: 'middle' });
      } else {
        out.push({ at: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2], kind: 'middle' });
        edges.push([a, b]);
      }
    }
    if (contour.closed) {
      const box = boxOf(points);
      out.push({ at: [(box[0] + box[2]) / 2, (box[1] + box[3]) / 2], kind: 'shape' });
    }
  }
  if (bounds) {
    for (const fx of [0, 0.5, 1]) for (const fy of [0, 0.5, 1]) {
      out.push({ at: [bounds.min.x + fx * (bounds.max.x - bounds.min.x), bounds.min.y + fy * (bounds.max.y - bounds.min.y)], kind: 'part' });
    }
  }
  if (edges.length <= CROSSING_EDGES) {
    for (let i = 0; i < edges.length; i++) for (let j = i + 1; j < edges.length; j++) {
      const x = crossing(edges[i]!, edges[j]!);
      if (x) out.push({ at: x, kind: 'crossing' });
    }
  }
  return out;
}

/** The candidate nearest `at` within `reach` millimetres, or `null`. */
export function snapOrigin(at: XY, candidates: Snap[], reach: number): Snap | null {
  let best: Snap | null = null;
  let nearest = reach;
  for (const c of candidates) {
    const d = dist(c.at, at);
    if (d <= nearest) { nearest = d; best = c; }
  }
  return best;
}

const dist = (a: XY, b: XY): number => Math.hypot(a[0] - b[0], a[1] - b[1]);

function boxOf(points: XY[]): [number, number, number, number] {
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (const [x, y] of points) { minX = Math.min(minX, x); maxX = Math.max(maxX, x); minY = Math.min(minY, y); maxY = Math.max(maxY, y); }
  return [minX, minY, maxX, maxY];
}

/** Where the line turns by more than 20°: an arc's many short steps turn
 *  by less and are not corners. */
function cornerIndices(points: XY[], closed: boolean): number[] {
  const n = points.length;
  const out: number[] = [];
  for (let i = closed ? 0 : 1; i < (closed ? n : n - 1); i++) {
    const a = points[(i - 1 + n) % n]!, b = points[i]!, c = points[(i + 1) % n]!;
    const u = [b[0] - a[0], b[1] - a[1]] as const, v = [c[0] - b[0], c[1] - b[1]] as const;
    const lu = Math.hypot(u[0], u[1]), lv = Math.hypot(v[0], v[1]);
    if (lu < 1e-9 || lv < 1e-9) continue;
    if ((u[0] * v[0] + u[1] * v[1]) / (lu * lv) < Math.cos(20 * Math.PI / 180)) out.push(i);
  }
  return out;
}

/** Lines with more points than this are not searched for arcs. */
const ARC_POINTS = 20000;

/** The line cut into straight edges and arcs, never across a corner: a
 *  slot's sides and its round ends meet without one. */
function runs(points: XY[], turns: number[], closed: boolean): XY[][] {
  const n = points.length;
  // A closed line starts at a corner when it has one, and comes back to it.
  const start = closed && turns.length ? turns[0]! : 0;
  const seq = closed ? [...points.slice(start), ...points.slice(0, start), points[start]!] : points;
  const corner = new Set(closed ? turns.map((i) => (i - start + n) % n) : turns);
  const out: XY[][] = [];
  let i = 0;
  while (i < seq.length - 1) {
    let e = i + 1;
    while (e + 1 < seq.length && !corner.has(e) && straight(seq.slice(i, e + 2))) e++;
    if (e === i + 1 && seq.length <= ARC_POINTS) {
      let best = -1;
      for (let end = i + 2; end < seq.length && !corner.has(end - 1); end++) {
        if (!arcCentre(seq.slice(i, end + 1))) break;
        best = end;
      }
      if (best > 0) e = best;
    }
    out.push(seq.slice(i, e + 1));
    i = e;
  }
  return out;
}

/** Whether every point lies on the line from the first to the last. */
function straight(run: XY[]): boolean {
  const a = run[0]!, b = run[run.length - 1]!;
  const length = dist(a, b);
  if (length < 1e-9) return false;
  return run.every((p) => Math.abs((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])) / length <= 0.02);
}

/** The centre of the circle through a stretch, when every point lies on
 *  it, its steps are alike and it bends: a rounded corner, a slot's end.
 *  A long straight edge beside an arc has a step unlike the arc's. */
function arcCentre(run: XY[]): XY | null {
  const steps = run.slice(1).map((p, i) => dist(p, run[i]!));
  if (Math.max(...steps) > 2.5 * Math.min(...steps)) return null;
  const a = run[0]!, b = run[Math.floor(run.length / 2)]!, c = run[run.length - 1]!;
  const d = 2 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
  const span = dist(a, c) || dist(a, b);
  if (Math.abs(d) < 1e-9 * Math.max(1, span * span)) return null;
  const a2 = a[0] ** 2 + a[1] ** 2, b2 = b[0] ** 2 + b[1] ** 2, c2 = c[0] ** 2 + c[1] ** 2;
  const centre: XY = [(a2 * (b[1] - c[1]) + b2 * (c[1] - a[1]) + c2 * (a[1] - b[1])) / d,
    (a2 * (c[0] - b[0]) + b2 * (a[0] - c[0]) + c2 * (b[0] - a[0])) / d];
  const r = dist(centre, a);
  // A nearly straight stretch has a huge radius: an edge, not an arc.
  if (r > 50 * span) return null;
  const tolerance = Math.max(0.02, r * 0.01);
  return run.every((p) => Math.abs(dist(centre, p) - r) <= tolerance) ? centre : null;
}

/** A closed contour whose points all lie at one distance from their
 *  middle is a circle: its centre. */
function circleCentre(points: XY[], closed: boolean): XY | null {
  if (!closed || points.length < 8) return null;
  const [minX, minY, maxX, maxY] = boxOf(points);
  const centre: XY = [(minX + maxX) / 2, (minY + maxY) / 2];
  const r = (maxX - minX + maxY - minY) / 4;
  if (r <= 0) return null;
  return points.every((p) => Math.abs(dist(p, centre) - r) <= Math.max(0.02, r * 0.01)) ? centre : null;
}

/** Where two straight edges cross, both within their length. Edges that
 *  only meet end to end share a corner, already offered. */
function crossing([a, b]: [XY, XY], [c, d]: [XY, XY]): XY | null {
  const r = [b[0] - a[0], b[1] - a[1]] as const, s = [d[0] - c[0], d[1] - c[1]] as const;
  const den = r[0] * s[1] - r[1] * s[0];
  if (Math.abs(den) < 1e-12) return null;
  const t = ((c[0] - a[0]) * s[1] - (c[1] - a[1]) * s[0]) / den;
  const u = ((c[0] - a[0]) * r[1] - (c[1] - a[1]) * r[0]) / den;
  const inside = (v: number) => v > 1e-6 && v < 1 - 1e-6;
  return inside(t) && inside(u) ? [a[0] + t * r[0], a[1] + t * r[1]] : null;
}
