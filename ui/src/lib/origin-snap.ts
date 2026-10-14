// Where a tap on the drawing puts the origin: the nearest corner or
// circle centre within reach, else the tapped point itself.
import type { PreviewContour } from '../api';

type XY = [number, number];

/** The corners and circle centres of the drawn contours, as cut. */
export function originCandidates(contours: PreviewContour[]): XY[] {
  const out: XY[] = [];
  for (const contour of contours) {
    const points = contour.paths.filter((p) => p.kind === 'cut' || p.kind === 'film').flatMap((p) => p.points);
    // A closed line ends where it began: once is enough.
    const [first, last] = [points[0], points[points.length - 1]];
    if (contour.closed && first && last && points.length > 2 && Math.hypot(first[0] - last[0], first[1] - last[1]) < 1e-6) points.pop();
    if (points.length < 2) continue;
    out.push(...corners(points, contour.closed));
    const centre = circleCentre(points, contour.closed);
    if (centre) out.push(centre);
  }
  return out;
}

/** The candidate nearest `at` within `reach` millimetres, else `at`. */
export function snapOrigin(at: XY, candidates: XY[], reach: number): XY {
  let best: XY = at;
  let nearest = reach;
  for (const c of candidates) {
    const d = Math.hypot(c[0] - at[0], c[1] - at[1]);
    if (d <= nearest) { nearest = d; best = c; }
  }
  return best;
}

/** Points where the line turns by more than 20°: an arc's many short
 *  steps turn by less and are not corners. Open ends count too. */
function corners(points: XY[], closed: boolean): XY[] {
  const n = points.length;
  const out: XY[] = [];
  if (!closed) out.push(points[0]!, points[n - 1]!);
  for (let i = closed ? 0 : 1; i < (closed ? n : n - 1); i++) {
    const a = points[(i - 1 + n) % n]!, b = points[i]!, c = points[(i + 1) % n]!;
    const u = [b[0] - a[0], b[1] - a[1]], v = [c[0] - b[0], c[1] - b[1]];
    const lu = Math.hypot(u[0]!, u[1]!), lv = Math.hypot(v[0]!, v[1]!);
    if (lu < 1e-9 || lv < 1e-9) continue;
    const cos = (u[0]! * v[0]! + u[1]! * v[1]!) / (lu * lv);
    if (cos < Math.cos(20 * Math.PI / 180)) out.push(b);
  }
  return out;
}

/** A closed contour whose points all lie at one distance from their
 *  middle is a circle: its centre. */
function circleCentre(points: XY[], closed: boolean): XY | null {
  if (!closed || points.length < 8) return null;
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (const [x, y] of points) { minX = Math.min(minX, x); maxX = Math.max(maxX, x); minY = Math.min(minY, y); maxY = Math.max(maxY, y); }
  const centre: XY = [(minX + maxX) / 2, (minY + maxY) / 2];
  const r = (maxX - minX + maxY - minY) / 4;
  if (r <= 0) return null;
  const round = points.every(([x, y]) => Math.abs(Math.hypot(x - centre[0], y - centre[1]) - r) <= Math.max(0.02, r * 0.01));
  return round ? centre : null;
}
