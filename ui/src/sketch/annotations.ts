// What dimensions measure and how they are drawn, and where constraint
// glyphs sit: all in sketch millimetres, for the canvas to turn into pixels.
import { add, angleOf, dist, dot, midOf, polar, scale, shapeOf, sub, sweep, type Vec } from './geometry';
import { isDimension, point, type Constraint, type Dimension, type Entity, type Id, type Sketch } from './model';

const entityOf = (s: Sketch, id: Id): Entity | undefined => s.entities.find((e) => e.id === id);

/** The direction of a line, from its first point to its second. */
function direction(s: Sketch, line: Id): { a: Vec; b: Vec; u: Vec } | null {
  const l = entityOf(s, line);
  if (l?.kind !== 'line') return null;
  const a = point(s, l.a), b = point(s, l.b);
  const d = dist(a, b) || 1;
  return { a, b, u: { x: (b.x - a.x) / d, y: (b.y - a.y) / d } };
}

/** Where two lines meet, taken as infinite; null when parallel. */
function meet(p: Vec, u: Vec, q: Vec, v: Vec): Vec | null {
  const den = u.x * v.y - u.y * v.x;
  if (Math.abs(den) < 1e-12) return null;
  const t = ((q.x - p.x) * v.y - (q.y - p.y) * v.x) / den;
  return add(p, scale(u, t));
}

/** What a dimension measures in the sketch as it stands. */
export function measure(s: Sketch, d: Dimension): number {
  switch (d.kind) {
    case 'distance': {
      const a = point(s, d.a), b = point(s, d.b);
      return d.axis === 'horizontal' ? Math.abs(b.x - a.x) : d.axis === 'vertical' ? Math.abs(b.y - a.y) : dist(a, b);
    }
    case 'offset': {
      const l = direction(s, d.line);
      if (!l) return 0;
      const p = point(s, d.point);
      return Math.abs((p.x - l.a.x) * l.u.y - (p.y - l.a.y) * l.u.x);
    }
    case 'diameter': case 'radius': {
      const e = entityOf(s, d.curve);
      if (!e || e.kind === 'line' || e.kind === 'spline') return 0;
      const r = e.kind === 'arc' ? dist(point(s, e.c), point(s, e.a)) : e.r;
      return d.kind === 'diameter' ? 2 * r : r;
    }
    case 'angle': {
      const a = direction(s, d.a), b = direction(s, d.b);
      if (!a || !b) return 0;
      let t = Math.abs(angleOf({ x: 0, y: 0 }, b.u) - angleOf({ x: 0, y: 0 }, a.u)) % (2 * Math.PI);
      if (t > Math.PI) t = 2 * Math.PI - t;
      return (t * 180) / Math.PI;
    }
  }
}

/** A drawn dimension: its dimension line or arc, extension lines, arrow
 *  tips with the direction they point, and where its text goes. */
export interface Drawing {
  lines: Array<[Vec, Vec]>;
  arc: { c: Vec; r: number; start: number; end: number } | null;
  arrows: Array<{ at: Vec; toward: Vec }>;
  text: Vec;
}

/** The point a dimension's text hangs on: between the points it measures,
 *  a point's foot, a circle's centre, the corner of an angle. */
export function labelAnchor(s: Sketch, d: Dimension): Vec {
  switch (d.kind) {
    case 'distance': { const a = point(s, d.a), b = point(s, d.b); return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }; }
    case 'offset': return point(s, d.point);
    case 'diameter': case 'radius': { const e = entityOf(s, d.curve); return e && 'c' in e ? point(s, e.c) : { x: 0, y: 0 }; }
    case 'angle': {
      const a = direction(s, d.a), b = direction(s, d.b);
      return a && b ? meet(a.a, a.u, b.a, b.u) ?? a.a : { x: 0, y: 0 };
    }
  }
}

/** Where a dimension's text is, in sketch millimetres. */
export function labelAt(s: Sketch, d: Dimension): Vec {
  const o = labelAnchor(s, d);
  return { x: o.x + d.at[0], y: o.y + d.at[1] };
}

/** How a dimension is drawn, from where its text was placed. */
export function draw(s: Sketch, d: Dimension): Drawing | null {
  const at = labelAt(s, d);
  switch (d.kind) {
    case 'distance': {
      const p = point(s, d.a), q = point(s, d.b);
      let A: Vec, B: Vec;
      if (d.axis === 'horizontal') { A = { x: p.x, y: at.y }; B = { x: q.x, y: at.y }; }
      else if (d.axis === 'vertical') { A = { x: at.x, y: p.y }; B = { x: at.x, y: q.y }; }
      else {
        const L = dist(p, q) || 1;
        const n = { x: -(q.y - p.y) / L, y: (q.x - p.x) / L };
        const off = dot(sub(at, p), n);
        A = add(p, scale(n, off)); B = add(q, scale(n, off));
      }
      return { lines: [[p, A], [q, B], [A, B]], arc: null, arrows: [{ at: A, toward: B }, { at: B, toward: A }], text: at };
    }
    case 'offset': {
      const l = direction(s, d.line);
      if (!l) return null;
      const p = point(s, d.point);
      const foot = add(l.a, scale(l.u, dot(sub(p, l.a), l.u)));
      return { lines: [[p, foot]], arc: null, arrows: [{ at: p, toward: foot }, { at: foot, toward: p }], text: at };
    }
    case 'diameter': case 'radius': {
      const e = entityOf(s, d.curve);
      if (!e || e.kind === 'line' || e.kind === 'spline') return null;
      const c = point(s, e.c);
      const r = e.kind === 'arc' ? dist(c, point(s, e.a)) : e.r;
      const u = dist(at, c) > 1e-9 ? scale(sub(at, c), 1 / dist(at, c)) : { x: 1, y: 0 };
      const far = add(c, scale(u, r));
      if (d.kind === 'radius') return { lines: [[c, far], [far, at]], arc: null, arrows: [{ at: far, toward: c }], text: at };
      const near = sub(c, scale(u, r));
      return { lines: [[near, far], [far, at]], arc: null, arrows: [{ at: near, toward: far }, { at: far, toward: near }], text: at };
    }
    case 'angle': {
      const a = direction(s, d.a), b = direction(s, d.b);
      if (!a || !b) return null;
      const o = meet(a.a, a.u, b.a, b.u) ?? a.a;
      const r = Math.max(dist(at, o), 1e-3);
      const toward = angleOf(o, at);
      // Each line's direction from the corner nearer the text's side.
      const pick = (u: Vec) => {
        const t = angleOf({ x: 0, y: 0 }, u);
        const back = t + Math.PI;
        const off = (x: number) => Math.abs(((x - toward + 3 * Math.PI) % (2 * Math.PI)) - Math.PI);
        return off(t) <= off(back) ? t : back;
      };
      let start = pick(a.u), end = pick(b.u);
      if (sweep(start, end) > Math.PI) [start, end] = [end, start];
      const s0 = polar(o, r, start), e0 = polar(o, r, end);
      return { lines: [], arc: { c: o, r, start, end }, arrows: [{ at: s0, toward: polar(o, r, start + 0.2) }, { at: e0, toward: polar(o, r, end - 0.2) }], text: at };
    }
  }
}

/** A dimension's value as written: millimetres, ⌀ and R, degrees. */
export function label(d: Dimension, format: (mm: number) => string): string {
  const text = d.kind === 'angle' ? `${Number(d.value.toFixed(2))}°`
    : d.kind === 'diameter' ? `⌀${format(d.value)}`
      : d.kind === 'radius' ? `R${format(d.value)}` : format(d.value);
  return d.driven ? `(${text})` : text;
}

/** One glyph for a relation: what it says and the sketch point it sits by. */
export interface Glyph { id: Id; text: string; at: Vec }

const GLYPH: Partial<Record<Constraint['kind'], string>> = {
  horizontal: 'H', vertical: 'V', parallel: '∥', perpendicular: '⊥', equal: '=', collinear: '≡',
  tangent: 'T', concentric: '◎', midpoint: 'Δ', on: '•', symmetric: '⇔', fix: '⚓',
};

/** Where a curve's glyph sits: a line's middle, an arc's middle, a circle's top. */
function anchorOf(s: Sketch, id: Id): Vec | null {
  const e = entityOf(s, id);
  if (!e) return null;
  const shape = shapeOf(s, e);
  return midOf(shape) ?? (shape.kind === 'circle' ? polar(shape.c, shape.r, Math.PI / 2) : null);
}

export function glyphs(s: Sketch): Glyph[] {
  const out: Glyph[] = [];
  const at = (c: Constraint, where: Vec | null) => { if (where) out.push({ id: c.id, text: GLYPH[c.kind]!, at: where }); };
  for (const c of s.constraints) {
    if (isDimension(c) || !GLYPH[c.kind]) continue;
    switch (c.kind) {
      case 'horizontal': case 'vertical': at(c, anchorOf(s, c.line)); break;
      case 'parallel': case 'perpendicular': case 'equal': case 'collinear': case 'tangent': case 'concentric':
        at(c, anchorOf(s, c.a)); at(c, anchorOf(s, c.b)); break;
      case 'midpoint': case 'on': case 'fix': at(c, point(s, c.point)); break;
      case 'symmetric': at(c, point(s, c.a)); at(c, point(s, c.b)); break;
    }
  }
  return out;
}

/** Fusion's choice of axis for a distance from where the text is being put:
 *  above or below the pair, horizontal; beside it, vertical; else along it. */
export function axisFor(p: Vec, q: Vec, at: Vec): 'aligned' | 'horizontal' | 'vertical' {
  const minX = Math.min(p.x, q.x), maxX = Math.max(p.x, q.x), minY = Math.min(p.y, q.y), maxY = Math.max(p.y, q.y);
  const insideX = at.x > minX && at.x < maxX, insideY = at.y > minY && at.y < maxY;
  if (insideX && !insideY) return 'horizontal';
  if (insideY && !insideX) return 'vertical';
  return 'aligned';
}

