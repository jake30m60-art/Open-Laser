// Fusion's modify tools on the sketch model: trim, extend, break, fillet,
// chamfer, offset, mirror and the patterns. Each works on a copy and
// returns it, or says why it cannot; the solver settles the result.
import {
  add, angleOf, dist, dot, intersections, len, normalize, polar, scale, shapeOf, sub, sweep, type Shape, type Vec,
} from './geometry';
import { addConstraint, addEntity, addPoint, clone, constraintRefs, isDimension, merge, point, pointsOf, remove, type Arc, type Circle, type Constraint, type Entity, type Id, type Line, type Sketch } from './model';

export type Outcome = { sketch: Sketch; created: Id[] } | { error: string };

const TAU = Math.PI * 2;
/** Positions closer than this are the same point. */
const SAME = 1e-6;

function entity(s: Sketch, id: Id): Entity {
  const e = s.entities.find((x) => x.id === id);
  if (!e) throw new Error(`sketch curve ${id} is missing`);
  return e;
}

/** The curves trim, extend, break, fillet and offset work on. */
type Basic = Line | Circle | Arc;
const isBasic = (e: Entity): e is Basic => e.kind === 'line' || e.kind === 'circle' || e.kind === 'arc';
/** Why an ellipse or spline is left as it is. */
const NOT_BASIC = 'This works on lines, arcs and circles; ellipses and splines are moved, copied, mirrored or patterned whole.';

/** A basic curve's shape: a line, circle or arc, never a polyline. */
type Flat = Exclude<Shape, { kind: 'poly' }>;
const flat = (s: Sketch, e: Basic): Flat => shapeOf(s, e) as Flat;

function basic(s: Sketch, id: Id): Basic {
  const e = entity(s, id);
  if (!isBasic(e)) throw new Error(NOT_BASIC);
  return e;
}

/** How many curves run through a point. */
const uses = (s: Sketch, p: Id): number => s.entities.filter((e) => pointsOf(e).includes(p)).length;

/** An existing curve end or centre at `at`, or a new point there. */
function pointNear(s: Sketch, at: Vec): { id: Id; existing: boolean } {
  const found = s.points.find((p) => dist(p, at) < SAME);
  return found ? { id: found.id, existing: true } : { id: addPoint(s, at.x, at.y), existing: false };
}

/** A new end where a curve meets `by`: shared when a point is already
 *  there, otherwise held on `by` by an on-curve constraint. */
function endOn(s: Sketch, at: Vec, by: Id | null): Id {
  const p = pointNear(s, at);
  if (!p.existing && by !== null) addConstraint(s, { kind: 'on', point: p.id, curve: by });
  return p.id;
}

/** Where along a curve a position lies: a line's fraction, an arc's swept
 *  angle from its start, a circle's angle. */
function paramOf(shape: Shape, p: Vec): number {
  if (shape.kind === 'poly') return 0;
  if (shape.kind === 'line') {
    const d = sub(shape.b, shape.a);
    return dot(sub(p, shape.a), d) / dot(d, d);
  }
  const a = angleOf(shape.c, p);
  return shape.kind === 'circle' ? normalize(a) : normalize(a - shape.start);
}

function atParam(shape: Shape, t: number): Vec {
  if (shape.kind === 'poly') return shape.pts[0]!;
  if (shape.kind === 'line') return add(shape.a, scale(sub(shape.b, shape.a), t));
  return polar(shape.c, shape.r, shape.kind === 'circle' ? t : shape.start + t);
}

/** The far end of a curve's parameter range. */
const span = (shape: Shape): number => (shape.kind === 'line' || shape.kind === 'poly' ? 1 : shape.kind === 'arc' ? sweep(shape.start, shape.end) : TAU);

/** Where other curves cross this one, by parameter, inside its range. */
function crossings(s: Sketch, id: Id): Array<{ t: number; at: Vec; by: Id }> {
  const shape = shapeOf(s, entity(s, id));
  const out: Array<{ t: number; at: Vec; by: Id }> = [];
  for (const other of s.entities) {
    if (other.id === id) continue;
    for (const at of intersections(shape, shapeOf(s, other))) {
      const t = paramOf(shape, at);
      if (t > -1e-9 && t < span(shape) + 1e-9) out.push({ t, at, by: other.id });
    }
  }
  out.sort((a, b) => a.t - b.t);
  return out.filter((c, i) => i === 0 || Math.abs(c.t - out[i - 1]!.t) > 1e-9);
}

/** Constraints that only make sense for the whole of a curve that is being cut up. */
function dropSizes(s: Sketch, id: Id): void {
  s.constraints = s.constraints.filter((c) => !((c.kind === 'diameter' || c.kind === 'radius') && c.curve === id));
}

/** A line whose length an edit changed lets go of what held its length:
 *  being equal to other lines and having a point at its middle. Kept, they
 *  would pull every other side of the outline after it. */
function freeLength(s: Sketch, ids: readonly Id[]): void {
  const lines = new Set(ids.filter((id) => s.entities.some((e) => e.id === id && e.kind === 'line')));
  s.constraints = s.constraints.filter((c) =>
    !(c.kind === 'equal' && (lines.has(c.a) || lines.has(c.b))) && !(c.kind === 'midpoint' && lines.has(c.line)));
}

/** Copies a line's horizontal or vertical constraint to a piece of it. */
function copyAxis(s: Sketch, from: Id, to: Id): void {
  for (const c of s.constraints.filter((c) => (c.kind === 'horizontal' || c.kind === 'vertical') && c.line === from)) {
    addConstraint(s, { kind: c.kind as 'horizontal' | 'vertical', line: to });
  }
}

/** The bounds around a click along a curve: the nearest crossings either side. */
function around(s: Sketch, id: Id, click: Vec) {
  const shape = shapeOf(s, entity(s, id));
  const cuts = crossings(s, id).filter((c) => shape.kind === 'circle' || (c.t > 1e-9 && c.t < span(shape) - 1e-9));
  const t = paramOf(shape, click);
  if (shape.kind === 'circle') {
    if (cuts.length < 2) return { shape, cuts, lo: null, hi: null };
    const after = cuts.find((c) => c.t > t) ?? cuts[0]!;
    const before = [...cuts].reverse().find((c) => c.t < t) ?? cuts[cuts.length - 1]!;
    return { shape, cuts, lo: before, hi: after };
  }
  const tc = Math.min(Math.max(t, 0), span(shape));
  return { shape, cuts, lo: [...cuts].reverse().find((c) => c.t < tc) ?? null, hi: cuts.find((c) => c.t > tc) ?? null };
}

/** The stretch of a curve a trim at the click would remove, for previewing it. */
export function stretch(s: Sketch, id: Id, click: Vec): Shape | null {
  const e = s.entities.find((x) => x.id === id);
  if (!e || !isBasic(e)) return null;
  const { shape, lo, hi } = around(s, id, click);
  if (shape.kind === 'circle') {
    if (!lo || !hi) return shape;
    return { kind: 'arc', c: shape.c, r: shape.r, start: lo.t, end: hi.t };
  }
  const t0 = lo ? lo.t : 0, t1 = hi ? hi.t : span(shape);
  if (shape.kind === 'line') return { kind: 'line', a: atParam(shape, t0), b: atParam(shape, t1) };
  if (shape.kind === 'poly') return null;
  return { kind: 'arc', c: shape.c, r: shape.r, start: shape.start + t0, end: shape.start + t1 };
}

/** Removes the stretch of a curve between the crossings around the click. */
export function trim(source: Sketch, id: Id, click: Vec): Outcome {
  const s = clone(source);
  const found = entity(s, id);
  if (!isBasic(found)) return { error: NOT_BASIC };
  const e = found;
  const { lo, hi } = around(s, id, click);
  if (e.kind === 'circle') {
    if (!lo || !hi) { remove(s, [id]); return { sketch: s, created: [] }; }
    // What is left runs from the crossing after the click round to the one before it.
    const a = endOn(s, hi.at, hi.by), b = endOn(s, lo.at, lo.by);
    dropSizes(s, id);
    const arc = { kind: 'arc' as const, id: e.id, c: e.c, a, b, ...(e.construction ? { construction: true } : {}) };
    s.entities = s.entities.map((x) => (x.id === id ? arc : x));
    remove(s, []);
    return { sketch: s, created: [id] };
  }
  if (!lo && !hi) { remove(s, [id]); return { sketch: s, created: [] }; }
  freeLength(s, [id]);
  if (!lo) {
    const p = endOn(s, hi!.at, hi!.by);
    e.a = p;
  } else if (!hi) {
    const p = endOn(s, lo.at, lo.by);
    e.b = p;
  } else {
    const end = e.b;
    const p0 = endOn(s, lo.at, lo.by), p1 = endOn(s, hi.at, hi.by);
    e.b = p0;
    const piece = e.kind === 'line'
      ? addEntity(s, { kind: 'line', a: p1, b: end, ...(e.construction ? { construction: true } : {}) })
      : addEntity(s, { kind: 'arc', c: e.c, a: p1, b: end, ...(e.construction ? { construction: true } : {}) });
    if (e.kind === 'line') copyAxis(s, id, piece);
    else addConstraint(s, { kind: 'equal', a: id, b: piece });
    remove(s, []);
    return { sketch: s, created: [id, piece] };
  }
  remove(s, []);
  return { sketch: s, created: [id] };
}

/** Splits a curve at the crossings around the click, so that stretch stands alone. */
export function breakAt(source: Sketch, id: Id, click: Vec): Outcome {
  const s = clone(source);
  const found = entity(s, id);
  if (!isBasic(found)) return { error: NOT_BASIC };
  const e = found;
  const { lo, hi } = around(s, id, click);
  if (!lo && !hi) return { error: 'Nothing crosses this curve there to break it at.' };
  const flag = e.construction ? { construction: true } : {};
  if (e.kind === 'circle') {
    if (!lo || !hi) return { error: 'A circle breaks between two crossings.' };
    const a = endOn(s, lo.at, lo.by), b = endOn(s, hi.at, hi.by);
    dropSizes(s, id);
    s.entities = s.entities.map((x) => (x.id === id ? { kind: 'arc' as const, id, c: e.c, a, b, ...flag } : x));
    const rest = addEntity(s, { kind: 'arc', c: e.c, a: b, b: a, ...flag });
    addConstraint(s, { kind: 'equal', a: id, b: rest });
    return { sketch: s, created: [id, rest] };
  }
  const created: Id[] = [id];
  freeLength(s, [id]);
  const end = e.b;
  const cutAt = [lo, hi].filter((c): c is NonNullable<typeof c> => !!c).map((c) => endOn(s, c.at, c.by));
  // The original keeps the first piece; each cut starts the next.
  e.b = cutAt[0]!;
  const ends = [...cutAt.slice(1), end];
  let from = cutAt[0]!;
  for (const to of ends) {
    const piece = e.kind === 'line' ? addEntity(s, { kind: 'line', a: from, b: to, ...flag }) : addEntity(s, { kind: 'arc', c: e.c, a: from, b: to, ...flag });
    if (e.kind === 'line') copyAxis(s, id, piece);
    else addConstraint(s, { kind: 'equal', a: id, b: piece });
    created.push(piece);
    from = to;
  }
  dropSizes(s, id);
  return { sketch: s, created };
}

/** Lengthens a line or arc from the end nearer the click to the next curve it meets. */
export function extend(source: Sketch, id: Id, click: Vec): Outcome {
  const s = clone(source);
  const found = entity(s, id);
  if (!isBasic(found)) return { error: NOT_BASIC };
  const e = found;
  if (e.kind === 'circle') return { error: 'A circle has no end to extend.' };
  const atEnd = dist(click, point(s, e.b)) <= dist(click, point(s, e.a));
  const endId = atEnd ? e.b : e.a;
  const end = point(s, endId);
  let best: { at: Vec; by: Id; d: number } | null = null;
  if (e.kind === 'line') {
    const other = point(s, atEnd ? e.a : e.b);
    const u = scale(sub(end, other), 1 / (dist(end, other) || 1));
    const ray: Shape = { kind: 'line', a: end, b: add(end, scale(u, 1e5)) };
    for (const x of s.entities) {
      if (x.id === id) continue;
      for (const at of intersections(ray, shapeOf(s, x))) {
        const d = dist(at, end);
        if (d > SAME && (!best || d < best.d)) best = { at, by: x.id, d };
      }
    }
  } else {
    const c = point(s, e.c), r = dist(c, point(s, e.a));
    const full: Shape = { kind: 'circle', c, r };
    const from = angleOf(c, end);
    for (const x of s.entities) {
      if (x.id === id) continue;
      for (const at of intersections(full, shapeOf(s, x))) {
        // Onwards from the end: counter-clockwise past b, clockwise before a.
        const turn = atEnd ? normalize(angleOf(c, at) - from) : normalize(from - angleOf(c, at));
        const own = shapeOf(s, e);
        if (turn > 1e-9 && own.kind === 'arc' && turn < TAU - sweep(own.start, own.end) && (!best || turn < best.d)) best = { at, by: x.id, d: turn };
      }
    }
  }
  if (!best) return { error: 'Nothing lies ahead of that end to extend to.' };
  // A shared end stays with the other curves; this one gets an end of its own.
  const shared = uses(s, endId) > 1;
  if (!shared) s.constraints = s.constraints.filter((c) => !(c.kind === 'distance' && (c.a === endId || c.b === endId)));
  freeLength(s, [id]);
  const target = s.points.find((p) => dist(p, best!.at) < SAME);
  if (target) {
    if (atEnd) e.b = target.id; else e.a = target.id;
  } else {
    // Detach old endpoint references before attaching the extended end.
    const p = endOn(s, best.at, best.by);
    if (atEnd) e.b = p; else e.a = p;
  }
  remove(s, []);
  return { sketch: s, created: [id] };
}

/** The two lines that meet at a point, as Fusion's fillet and chamfer take them. */
function corner(s: Sketch, vertex: Id): { lines: [Id, Id]; far: [Vec, Vec] } | { error: string } {
  // Lines that end at the same place through points of their own, as two
  // separate mirrors leave them, meet there all the same.
  const v = point(s, vertex);
  const twins = s.points.filter((p) => p.id !== vertex && dist(p, v) < SAME);
  for (const other of twins) merge(s, other.id, vertex);
  if (twins.length) {
    // Each mirror tied its own copy; once they are one point, one tie is enough.
    const seen = new Set<string>();
    s.constraints = s.constraints.filter((c) => {
      if (isDimension(c)) return true;
      const key = `${c.kind}:${constraintRefs(c).join(',')}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }
  let lines = s.entities.filter((e) => e.kind === 'line' && !e.construction && (e.a === vertex || e.b === vertex));
  if (lines.length > 2) {
    // A mirror line ending at the corner is not one of its sides: the
    // corner is between a line and its mirror image.
    const axes = new Set(s.constraints.flatMap((c) => (c.kind === 'symmetric' ? [c.line] : [])));
    const sides = lines.filter((l) => !axes.has(l.id));
    if (sides.length === 2) lines = sides;
  }
  if (lines.length !== 2) return { error: 'Pick a corner where exactly two lines meet.' };
  const farEnd = (l: Entity): Vec => (l.kind === 'line' ? point(s, l.a === vertex ? l.b : l.a) : point(s, vertex));
  const far: [Vec, Vec] = [farEnd(lines[0]!), farEnd(lines[1]!)];
  return { lines: [lines[0]!.id, lines[1]!.id], far };
}

/** Moves one end of a line from `from` to the new point `to`. */
function reend(s: Sketch, line: Id, from: Id, to: Id): void {
  const l = entity(s, line);
  if (l.kind !== 'line') return;
  if (l.a === from) l.a = to; else if (l.b === from) l.b = to;
}

/** Rounds the corner at `vertex` with a tangent arc of radius `r`. */
export function fillet(source: Sketch, vertex: Id, r: number): Outcome {
  if (!(r > 0)) return { error: 'The radius must be above zero.' };
  const s = clone(source);
  const found = corner(s, vertex);
  if ('error' in found) return curveFillet(s, vertex, r) ?? found;
  const v = point(s, vertex);
  const u1 = scale(sub(found.far[0], v), 1 / dist(found.far[0], v));
  const u2 = scale(sub(found.far[1], v), 1 / dist(found.far[1], v));
  const half = Math.acos(Math.max(-1, Math.min(1, dot(u1, u2)))) / 2;
  if (half < 1e-6 || Math.PI / 2 - half < 1e-6) return { error: 'The lines run straight on there; there is no corner to round.' };
  const d = r / Math.tan(half);
  if (d >= dist(found.far[0], v) - SAME || d >= dist(found.far[1], v) - SAME) return { error: 'The radius is too large for these lines.' };
  const t1 = add(v, scale(u1, d)), t2 = add(v, scale(u2, d));
  const bis = scale(add(u1, u2), 1 / len(add(u1, u2)));
  const center = add(v, scale(bis, r / Math.sin(half)));
  const p1 = addPoint(s, t1.x, t1.y), p2 = addPoint(s, t2.x, t2.y), c = addPoint(s, center.x, center.y);
  freeLength(s, found.lines);
  reend(s, found.lines[0], vertex, p1);
  reend(s, found.lines[1], vertex, p2);
  // The fillet is the short way round between the tangent points.
  const forward = sweep(angleOf(center, t1), angleOf(center, t2)) < Math.PI;
  const arc = addEntity(s, { kind: 'arc', c, a: forward ? p1 : p2, b: forward ? p2 : p1 });
  addConstraint(s, { kind: 'tangent', a: found.lines[0], b: arc });
  addConstraint(s, { kind: 'tangent', a: found.lines[1], b: arc });
  // The value sits outside the corner, away from the centre.
  const label = scale(bis, -r * 1.6);
  addConstraint(s, { kind: 'radius', curve: arc, value: r, at: [label.x, label.y] });
  remove(s, []);
  return { sketch: s, created: [arc] };
}

/** One side of a corner that is not two lines: a line or an arc ending at
 *  the corner, with the way it leaves the corner. */
interface Side { id: Id; kind: 'line' | 'arc'; dir: Vec; far: Vec; c?: Vec; r?: number }

/** Rounds a corner where a line meets an arc, or two arcs meet: the fillet's
 *  centre is `r` off each side towards the inside of the corner, its ends
 *  are the tangent points, and each side now ends there. */
function curveFillet(s: Sketch, vertex: Id, r: number): Outcome | null {
  const v = point(s, vertex);
  const curves = s.entities.filter((e) => (e.kind === 'line' || e.kind === 'arc') && !e.construction && (e.a === vertex || e.b === vertex));
  if (curves.length !== 2 || curves.every((e) => e.kind === 'line')) return null;
  const sides: Side[] = curves.map((e) => {
    if (e.kind === 'line') {
      const far = point(s, e.a === vertex ? e.b : e.a);
      return { id: e.id, kind: 'line', dir: scale(sub(far, v), 1 / dist(far, v)), far };
    }
    const arc = e as Arc;
    const c = point(s, arc.c), radius = dist(c, v), out = sub(v, c);
    // Leaving its start an arc turns counter-clockwise, leaving its end clockwise.
    const dir = arc.a === vertex ? { x: -out.y / radius, y: out.x / radius } : { x: out.y / radius, y: -out.x / radius };
    return { id: e.id, kind: 'arc', dir, far: point(s, arc.a === vertex ? arc.b : arc.a), c, r: radius };
  });
  const sum = add(sides[0]!.dir, sides[1]!.dir);
  if (len(sum) < 1e-9) return { error: 'The curves run straight on there; there is no corner to round.' };
  const bis = scale(sum, 1 / len(sum));
  // Where the fillet's centre may lie for each side: a line moved by r, or a circle r further in or out.
  type Locus = { kind: 'line'; p: Vec; d: Vec } | { kind: 'circle'; c: Vec; r: number };
  const locus = (side: Side): Locus => {
    if (side.kind === 'line') {
      const n = { x: -side.dir.y, y: side.dir.x };
      const towards = dot(n, bis) >= 0 ? 1 : -1;
      return { kind: 'line', p: add(v, scale(n, r * towards)), d: side.dir };
    }
    const inside = dot(sub(side.c!, v), bis) > 0;
    return { kind: 'circle', c: side.c!, r: inside ? side.r! - r : side.r! + r };
  };
  const meet = (a: Locus, b: Locus): Vec[] => {
    if (a.kind === 'circle' && b.kind === 'line') return meet(b, a);
    if (a.kind === 'line' && b.kind === 'circle') {
      const f = sub(a.p, b.c), B = 2 * dot(f, a.d), C = dot(f, f) - b.r * b.r, D = B * B - 4 * C;
      if (D < 0 || b.r <= 0) return [];
      return [(-B + Math.sqrt(D)) / 2, (-B - Math.sqrt(D)) / 2].map((t) => add(a.p, scale(a.d, t)));
    }
    if (a.kind === 'circle' && b.kind === 'circle') {
      const d = dist(a.c, b.c);
      if (d < 1e-9 || d > a.r + b.r || d < Math.abs(a.r - b.r) || a.r <= 0 || b.r <= 0) return [];
      const along = (a.r * a.r - b.r * b.r + d * d) / (2 * d), h = Math.sqrt(Math.max(0, a.r * a.r - along * along));
      const u = scale(sub(b.c, a.c), 1 / d), m = add(a.c, scale(u, along)), n = { x: -u.y, y: u.x };
      return [add(m, scale(n, h)), add(m, scale(n, -h))];
    }
    return [];
  };
  const guess = add(v, scale(bis, r));
  const centre = meet(locus(sides[0]!), locus(sides[1]!)).sort((p, q) => dist(p, guess) - dist(q, guess))[0];
  if (!centre) return { error: 'The radius does not fit between these curves.' };
  const touch = (side: Side): Vec => side.kind === 'line'
    ? add(v, scale(side.dir, dot(sub(centre, v), side.dir)))
    : add(side.c!, scale(sub(centre, side.c!), side.r! / dist(centre, side.c!)));
  const t = sides.map(touch);
  for (const [i, side] of sides.entries()) {
    // The tangent point must lie on the curve between the corner and its far end.
    if (dist(t[i]!, v) >= dist(side.far, v) - SAME && side.kind === 'line') return { error: 'The radius is too large for these curves.' };
  }
  const p1 = addPoint(s, t[0]!.x, t[0]!.y), p2 = addPoint(s, t[1]!.x, t[1]!.y), c = addPoint(s, centre.x, centre.y);
  freeLength(s, sides.filter((x) => x.kind === 'line').map((x) => x.id));
  for (const [i, side] of sides.entries()) {
    const e = entity(s, side.id);
    if (e.kind !== 'line' && e.kind !== 'arc') continue;
    const to = i === 0 ? p1 : p2;
    if (e.a === vertex) e.a = to; else if (e.b === vertex) e.b = to;
  }
  const forward = sweep(angleOf(centre, t[0]!), angleOf(centre, t[1]!)) < Math.PI;
  const arc = addEntity(s, { kind: 'arc', c, a: forward ? p1 : p2, b: forward ? p2 : p1 });
  addConstraint(s, { kind: 'tangent', a: sides[0]!.id, b: arc });
  addConstraint(s, { kind: 'tangent', a: sides[1]!.id, b: arc });
  const label = scale(bis, -r * 1.6);
  addConstraint(s, { kind: 'radius', curve: arc, value: r, at: [label.x, label.y] });
  remove(s, []);
  return { sketch: s, created: [arc] };
}

/** Cuts the corner at `vertex` with a line `d` back along each side. */
export function chamfer(source: Sketch, vertex: Id, d: number): Outcome {
  if (!(d > 0)) return { error: 'The distance must be above zero.' };
  const s = clone(source);
  const found = corner(s, vertex);
  if ('error' in found) return found;
  const v = point(s, vertex);
  if (d >= dist(found.far[0], v) - SAME || d >= dist(found.far[1], v) - SAME) return { error: 'The distance is too large for these lines.' };
  const q1 = add(v, scale(sub(found.far[0], v), d / dist(found.far[0], v)));
  const q2 = add(v, scale(sub(found.far[1], v), d / dist(found.far[1], v)));
  const p1 = addPoint(s, q1.x, q1.y), p2 = addPoint(s, q2.x, q2.y);
  freeLength(s, found.lines);
  reend(s, found.lines[0], vertex, p1);
  reend(s, found.lines[1], vertex, p2);
  const cut = addEntity(s, { kind: 'line', a: p1, b: p2 });
  remove(s, []);
  return { sketch: s, created: [cut] };
}

/** One curve of a chain, in the direction the chain runs. */
interface Link { id: Id; forward: boolean }

/** The selected lines and arcs as chains joined end to end, each in order. */
export function chains(s: Sketch, ids: readonly Id[]): Array<{ links: Link[]; closed: boolean }> {
  const open = new Set(ids.filter((id) => { const e = entity(s, id); return isBasic(e) && e.kind !== 'circle'; }));
  const ends = (id: Id): [Id, Id] => { const e = basic(s, id); return e.kind === 'circle' ? [e.c, e.c] : [e.a, e.b]; };
  const at = (p: Id) => [...open].filter((id) => ends(id).includes(p));
  const out: Array<{ links: Link[]; closed: boolean }> = [];
  while (open.size) {
    const first = open.values().next().value as Id;
    open.delete(first);
    const links: Link[] = [{ id: first, forward: true }];
    // Onwards from the first curve's end, then back from its start.
    let tail = ends(first)[1];
    for (let next = at(tail); next.length === 1; next = at(tail)) {
      const id = next[0]!;
      open.delete(id);
      const [a, b] = ends(id);
      links.push({ id, forward: a === tail });
      tail = a === tail ? b : a;
    }
    let head = ends(first)[0];
    for (let next = at(head); next.length === 1; next = at(head)) {
      const id = next[0]!;
      open.delete(id);
      const [a, b] = ends(id);
      links.unshift({ id, forward: b === head });
      head = b === head ? a : b;
    }
    out.push({ links, closed: head === tail && links.length > 1 });
  }
  return out;
}

/** Start and end of a link as it runs. */
function runEnds(s: Sketch, l: Link): [Vec, Vec] {
  const e = basic(s, l.id);
  if (e.kind === 'circle') { const c = point(s, e.c); return [c, c]; }
  const a = point(s, e.a), b = point(s, e.b);
  return l.forward ? [a, b] : [b, a];
}

/** Which side of a chain a position lies on: 1 left of the way it runs, -1 right. */
function sideOf(s: Sketch, links: Link[], at: Vec): number {
  let best = { d: Infinity, side: 1 };
  for (const l of links) {
    const shape = flat(s, basic(s, l.id));
    const near = shape.kind === 'line' ? atParam(shape, Math.max(0, Math.min(1, paramOf(shape, at)))) : polar(shape.c, shape.r, angleOf(shape.c, at));
    const d = dist(near, at);
    if (d >= best.d) continue;
    let side: number;
    if (shape.kind === 'line') {
      const [p, q] = runEnds(s, l);
      side = (q.x - p.x) * (at.y - p.y) - (q.y - p.y) * (at.x - p.x) >= 0 ? 1 : -1;
    } else {
      // Running counter-clockwise, the centre is on the left.
      const inside = dist(at, shape.c) < shape.r;
      side = inside === l.forward ? 1 : -1;
    }
    best = { d, side };
  }
  return best.side;
}

/** The offset of one link `d` to its left: a line moved across, an arc with a new radius. */
function offsetShape(s: Sketch, l: Link, d: number): Flat | null {
  const shape = flat(s, basic(s, l.id));
  if (shape.kind === 'line') {
    const [p, q] = runEnds(s, l);
    const n = scale({ x: -(q.y - p.y), y: q.x - p.x }, 1 / dist(p, q));
    const a = add(shape.a, scale(n, d)), b = add(shape.b, scale(n, d));
    return { kind: 'line', a, b };
  }
  const r = shape.r + (l.forward ? -d : d);
  return r > SAME ? { ...shape, r } : null;
}

/** Where two offset curves meet nearest `near`, both taken as infinite lines or full circles. */
function joint(s1: Flat, s2: Flat, near: Vec): Vec {
  const big = (x: Flat): Flat => {
    if (x.kind !== 'line') return { kind: 'circle', c: x.c, r: x.r };
    const u = scale(sub(x.b, x.a), 1e5 / (dist(x.a, x.b) || 1));
    return { kind: 'line', a: sub(x.a, u), b: add(x.b, u) };
  };
  const found = intersections(big(s1), big(s2));
  if (!found.length) return near;
  return found.reduce((best, p) => (dist(p, near) < dist(best, near) ? p : best));
}

/** Copies the selection `distance` to the side of `toward`: chains keep
 *  their corners, circles grow or shrink. */
export function offset(source: Sketch, ids: readonly Id[], distance: number, toward: Vec): Outcome {
  if (!(distance > 0)) return { error: 'The distance must be above zero.' };
  const s = clone(source);
  const created: Id[] = [];
  for (const id of ids) {
    const e = entity(s, id);
    if (e.kind !== 'circle') continue;
    const c = point(s, e.c);
    const r = e.r + (dist(toward, c) > e.r ? distance : -distance);
    if (r <= SAME) return { error: 'The offset would shrink a circle to nothing.' };
    created.push(addEntity(s, { kind: 'circle', c: addPoint(s, c.x, c.y), r }));
  }
  for (const chain of chains(s, ids)) {
    const d = distance * sideOf(s, chain.links, toward);
    const shapes = chain.links.map((l) => offsetShape(s, l, d));
    if (shapes.some((x) => !x)) return { error: 'The offset would shrink an arc to nothing.' };
    const offs = shapes as Flat[];
    // Each link's offset start and end as it runs, before the joints are found.
    const raw = chain.links.map((l, i): [Vec, Vec] => {
      const o = offs[i]!;
      if (o.kind === 'line') return l.forward ? [o.a, o.b] : [o.b, o.a];
      const [p, q] = runEnds(s, l);
      return [polar(o.c, o.r, angleOf(o.c, p)), polar(o.c, o.r, angleOf(o.c, q))];
    });
    const n = chain.links.length;
    const starts: Vec[] = raw.map((r) => r[0]);
    const endsAt: Vec[] = raw.map((r) => r[1]);
    for (let i = 0; i < n - 1 + (chain.closed ? 1 : 0); i++) {
      const j = (i + 1) % n;
      const at = dist(endsAt[i]!, starts[j]!) < SAME ? endsAt[i]! : joint(offs[i]!, offs[j]!, scale(add(endsAt[i]!, starts[j]!), 0.5));
      endsAt[i] = at;
      starts[j] = at;
    }
    const ids: Id[] = [];
    const shared = new Map<string, Id>();
    const pointFor = (v: Vec): Id => {
      const key = `${v.x.toFixed(9)},${v.y.toFixed(9)}`;
      const found = shared.get(key);
      if (found !== undefined) return found;
      const id = addPoint(s, v.x, v.y);
      shared.set(key, id);
      return id;
    };
    chain.links.forEach((l, i) => {
      const o = offs[i]!;
      const p = pointFor(starts[i]!), q = pointFor(endsAt[i]!);
      const [a, b] = l.forward ? [p, q] : [q, p];
      if (o.kind === 'line') {
        const id = addEntity(s, { kind: 'line', a, b }); ids.push(id);
        // Primary axes keep an offset rectangle rectangular when dimensioned.
        if (Math.abs(point(s,a).y-point(s,b).y) < SAME) addConstraint(s,{kind:'horizontal',line:id});
        else if (Math.abs(point(s,a).x-point(s,b).x) < SAME) addConstraint(s,{kind:'vertical',line:id});
      }
      else ids.push(addEntity(s, { kind: 'arc', c: addPoint(s, o.c.x, o.c.y), a, b }));
    });
    created.push(...ids);
  }
  if (!created.length) return { error: 'Select lines, arcs or circles to offset.' };
  return { sketch: s, created };
}

/** Relations worth carrying over to copies, when all they refer to was copied. */
const COPIED = new Set<Constraint['kind']>(['horizontal', 'vertical', 'parallel', 'perpendicular', 'equal', 'collinear', 'tangent', 'concentric', 'midpoint', 'on']);

/** Copies curves through `move`, with a point map so shared ends stay shared,
 *  and the relations among them unless `keepAxes` is false for H and V. */
function copy(s: Sketch, ids: readonly Id[], move: (v: Vec) => Vec, keepAxes: boolean, reverseArcs = false): Id[] {
  const map = new Map<Id, Id>();
  const pt = (id: Id): Id => {
    let to = map.get(id);
    if (to === undefined) { const v = move(point(s, id)); to = addPoint(s, v.x, v.y); map.set(id, to); }
    return to;
  };
  const curves = new Map<Id, Id>();
  for (const id of ids) {
    const e = entity(s, id);
    const flag = e.construction ? { construction: true } : {};
    let made: Id;
    if (e.kind === 'line') made = addEntity(s, { kind: 'line', a: pt(e.a), b: pt(e.b), ...flag });
    else if (e.kind === 'circle') made = addEntity(s, { kind: 'circle', c: pt(e.c), r: e.r, ...flag });
    else if (e.kind === 'ellipse') made = addEntity(s, { kind: 'ellipse', c: pt(e.c), m: pt(e.m), r: e.r, ...flag });
    else if (e.kind === 'ellipse-arc') made = addEntity(s, { kind: 'ellipse-arc', c: pt(e.c), m: pt(e.m), r: e.r, a: pt(reverseArcs ? e.b : e.a), b: pt(reverseArcs ? e.a : e.b), ...flag });
    else if (e.kind === 'spline') made = addEntity(s, { kind: 'spline', poles: e.poles.map(pt), degree: e.degree, knots: [...e.knots], ...(e.weights ? { weights: [...e.weights] } : {}), ...flag });
    else made = reverseArcs ? addEntity(s, { kind: 'arc', c: pt(e.c), a: pt(e.b), b: pt(e.a), ...flag }) : addEntity(s, { kind: 'arc', c: pt(e.c), a: pt(e.a), b: pt(e.b), ...flag });
    curves.set(id, made);
  }
  const mapped = (ref: Id) => curves.get(ref) ?? map.get(ref);
  for (const c of [...s.constraints]) {
    if (!COPIED.has(c.kind) || (!keepAxes && (c.kind === 'horizontal' || c.kind === 'vertical'))) continue;
    const next = { ...c } as Record<string, unknown>;
    let ok = true;
    for (const key of ['line', 'a', 'b', 'point', 'curve']) {
      if (!(key in next)) continue;
      const to = mapped(next[key] as Id);
      if (to === undefined) { ok = false; break; }
      next[key] = to;
    }
    if (ok) { delete next.id; addConstraint(s, next as never); }
  }
  return [...curves.values()];
}

/** Mirrors the selection across a line, tying each mirrored point to its original. */
export function mirror(source: Sketch, ids: readonly Id[], line: Id): Outcome {
  const s = clone(source);
  const l = entity(s, line);
  if (l.kind !== 'line') return { error: 'Mirror across a line.' };
  const picked = ids.filter((id) => id !== line);
  if (!picked.length) return { error: 'Select what to mirror first.' };
  const a = point(s, l.a), b = point(s, l.b);
  const u = scale(sub(b, a), 1 / dist(a, b));
  const reflect = (v: Vec): Vec => { const p = sub(v, a); const along = scale(u, dot(p, u)); return add(a, sub(scale(along, 2), p)); };
  const before = new Set(s.points.map((p) => p.id));
  // Points on the mirror line are their own mirror images and stay shared.
  const onLine = (v: Vec) => dist(reflect(v), v) < SAME;
  const map = new Map<Id, Id>();
  const pt = (id: Id): Id => {
    const v = point(s, id);
    if (onLine(v)) return id;
    let to = map.get(id);
    if (to === undefined) {
      const r = reflect(v);
      // An earlier mirror may have put a point there already: share it.
      to = s.points.find((p) => dist(p, r) < SAME)?.id ?? addPoint(s, r.x, r.y);
      map.set(id, to);
    }
    return to;
  };
  const created: Id[] = [];
  for (const id of picked) {
    const e = entity(s, id);
    const flag = e.construction ? { construction: true } : {};
    if (e.kind === 'line') created.push(addEntity(s, { kind: 'line', a: pt(e.a), b: pt(e.b), ...flag }));
    else if (e.kind === 'circle') {
      const made = addEntity(s, { kind: 'circle', c: pt(e.c), r: e.r, ...flag });
      addConstraint(s, { kind: 'equal', a: id, b: made });
      created.push(made);
    } else if (e.kind === 'ellipse') {
      created.push(addEntity(s, { kind: 'ellipse', c: pt(e.c), m: pt(e.m), r: e.r, ...flag }));
    } else if (e.kind === 'ellipse-arc') {
      // As for arcs, the mirror image runs the other way round.
      created.push(addEntity(s, { kind: 'ellipse-arc', c: pt(e.c), m: pt(e.m), r: e.r, a: pt(e.b), b: pt(e.a), ...flag }));
    } else if (e.kind === 'spline') {
      created.push(addEntity(s, { kind: 'spline', poles: e.poles.map(pt), degree: e.degree, knots: [...e.knots], ...(e.weights ? { weights: [...e.weights] } : {}), ...flag }));
    } else {
      // A mirror turns counter-clockwise into clockwise: the ends swap.
      const made = addEntity(s, { kind: 'arc', c: pt(e.c), a: pt(e.b), b: pt(e.a), ...flag });
      created.push(made);
    }
  }
  const arcCentres = new Set(picked.flatMap((id) => { const e = entity(s, id); return e.kind === 'arc' ? [e.c] : []; }));
  for (const [from, to] of map) {
    const tied = s.constraints.some((c) => c.kind === 'symmetric' && c.line === line && ((c.a === from && c.b === to) || (c.a === to && c.b === from)));
    if (before.has(from) && !arcCentres.has(from) && !tied) addConstraint(s, { kind: 'symmetric', a: from, b: to, line });
  }
  picked.forEach((id, i) => { if (entity(s, id).kind === 'arc') addConstraint(s, { kind: 'equal', a: id, b: created[i]! }); });
  return { sketch: s, created };
}

/** Copies the selection along X and Y: `nx` by `ny`, `dx` and `dy` apart. */
export function rectangularPattern(source: Sketch, ids: readonly Id[], nx: number, ny: number, dx: number, dy: number): Outcome {
  if (!ids.length) return { error: 'Select what to pattern first.' };
  if (nx < 1 || ny < 1 || nx * ny < 2) return { error: 'A pattern needs at least two copies.' };
  if (nx * ny * ids.length > 20000) return { error: 'That pattern would make more than 20 000 curves.' };
  const s = clone(source);
  const created: Id[] = [];
  for (let i = 0; i < nx; i++) {
    for (let j = 0; j < ny; j++) {
      if (i === 0 && j === 0) continue;
      created.push(...copy(s, ids, (v) => ({ x: v.x + i * dx, y: v.y + j * dy }), true));
    }
  }
  return { sketch: s, created };
}

/** Copies the selection round a centre: `count` in all over `angle` degrees,
 *  evenly round a full turn, ends included over less. */
export function circularPattern(source: Sketch, ids: readonly Id[], center: Vec, count: number, angle: number): Outcome {
  if (!ids.length) return { error: 'Select what to pattern first.' };
  if (count < 2) return { error: 'A pattern needs at least two copies.' };
  if (count * ids.length > 20000) return { error: 'That pattern would make more than 20 000 curves.' };
  const full = Math.abs(Math.abs(angle) - 360) < 1e-9;
  const step = ((full ? angle / count : angle / (count - 1)) * Math.PI) / 180;
  const s = clone(source);
  const created: Id[] = [];
  for (let k = 1; k < count; k++) {
    const t = k * step, cos = Math.cos(t), sin = Math.sin(t);
    const turn = (v: Vec): Vec => ({ x: center.x + (v.x - center.x) * cos - (v.y - center.y) * sin, y: center.y + (v.x - center.x) * sin + (v.y - center.y) * cos });
    created.push(...copy(s, ids, turn, false));
  }
  return { sketch: s, created };
}

