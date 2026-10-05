// Direct edits with the select tool, before constraints exist: moving
// points and curves, resizing circles, and keeping arcs whole when only
// some of their points move.
import { add, angleOf, dist, distanceTo, mid, shapeOf, sweep, type Vec } from './geometry';
import { point, pointsOf, type Id, type Sketch } from './model';

/** Moves points to `origin` plus `delta`; `origin` holds where each point started. */
export function translate(sketch: Sketch, origin: ReadonlyMap<Id, Vec>, delta: Vec): void {
  for (const [id, from] of origin) {
    const p = point(sketch, id);
    const to = add(from, delta);
    p.x = to.x;
    p.y = to.y;
  }
}

/**
 * Restores arcs some of whose points moved: the ends stay where they were
 * put and the centre follows, keeping the sweep each arc had in `before`.
 */
export function repairArcs(sketch: Sketch, before: Sketch): void {
  for (const e of sketch.entities) {
    if (e.kind !== 'arc') continue;
    const old = before.entities.find((o) => o.id === e.id);
    if (!old || old.kind !== 'arc') continue;
    const [c0, a0, b0] = [point(before, old.c), point(before, old.a), point(before, old.b)];
    const [c, a, b] = [point(sketch, e.c), point(sketch, e.a), point(sketch, e.b)];
    const shift = (p: Vec, q: Vec): Vec => ({ x: p.x - q.x, y: p.y - q.y });
    const da = shift(a, a0), db = shift(b, b0), dc = shift(c, c0);
    const same = (u: Vec, v: Vec): boolean => Math.abs(u.x - v.x) < 1e-9 && Math.abs(u.y - v.y) < 1e-9;
    if (same(da, dc) && same(db, dc)) continue;
    const theta = sweep(angleOf(c0, a0), angleOf(c0, b0));
    const half = dist(a, b) / 2;
    if (half < 1e-9) continue;
    // The centre sits on the chord's bisector, left of a→b for sweeps under half a turn.
    const m = mid(a, b);
    const n = { x: -(b.y - a.y) / (2 * half), y: (b.x - a.x) / (2 * half) };
    const d = half / Math.tan(theta / 2);
    c.x = m.x + n.x * d;
    c.y = m.y + n.y * d;
  }
}

/** The curve under the cursor, nearest first, within `reach` millimetres. */
export function hit(sketch: Sketch, at: Vec, reach: number): Id | null {
  let best: Id | null = null;
  let bestDistance = reach;
  for (const e of sketch.entities) {
    const d = distanceTo(shapeOf(sketch, e), at);
    if (d <= bestDistance) { best = e.id; bestDistance = d; }
  }
  return best;
}

/** The point handle under the cursor: a curve's end or centre. */
export function hitPoint(sketch: Sketch, at: Vec, reach: number): Id | null {
  let best: Id | null = null;
  let bestDistance = reach;
  for (const p of sketch.points) {
    const d = dist(p, at);
    if (d <= bestDistance) { best = p.id; bestDistance = d; }
  }
  return best;
}

/** Every curve connected to `start` through shared end points, as a double click selects a profile. */
export function chain(sketch: Sketch, start: Id): Id[] {
  const ends = (id: Id): Id[] => {
    const e = sketch.entities.find((x) => x.id === id);
    if (!e || e.kind === 'circle') return [];
    return e.kind === 'line' ? [e.a, e.b] : [e.a, e.b];
  };
  const seen = new Set<Id>([start]);
  const queue = [start];
  while (queue.length) {
    const id = queue.pop()!;
    const own = ends(id);
    for (const other of sketch.entities) {
      if (seen.has(other.id) || other.construction !== sketch.entities.find((x) => x.id === start)?.construction) continue;
      if (ends(other.id).some((p) => own.includes(p))) { seen.add(other.id); queue.push(other.id); }
    }
  }
  return [...seen];
}

/** Where every point of the given curves starts, for a drag. */
export function originsOf(sketch: Sketch, entities: Iterable<Id>): Map<Id, Vec> {
  const ids = new Set<Id>();
  for (const id of entities) {
    const e = sketch.entities.find((x) => x.id === id);
    if (e) for (const p of pointsOf(e)) ids.add(p);
  }
  return new Map([...ids].map((id) => { const p = point(sketch, id); return [id, { x: p.x, y: p.y }]; }));
}
