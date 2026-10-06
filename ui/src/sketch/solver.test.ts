import { beforeAll, describe, expect, it } from 'vitest';
import { addConstraint, addEntity, addPoint, emptySketch, point, type Sketch } from './model';
import { createTool } from './tools';
import { defined, freedom, initSolver, solve } from './solver';
import { angleOf, dist } from './geometry';
import type { Snap } from './snap';

beforeAll(async () => { await initSolver(); });

const free = (x: number, y: number): Snap => ({ x, y, kind: 'free' });

/** A 2-point rectangle from (0,0) to (w,h), as the tool draws it. */
function rectangle(w: number, h: number): Sketch {
  const s = emptySketch();
  const tool = createTool('rect2');
  tool.click(s, free(0, 0), {});
  tool.click(s, free(w, h), {});
  return s;
}

const lineIds = (s: Sketch) => s.entities.filter((e) => e.kind === 'line').map((e) => e.id);

describe('solver', () => {
  it('drives a rectangle to its dimensions and keeps it square', () => {
    const s = rectangle(40, 20);
    const [bottom, right] = lineIds(s) as [number, number];
    const b = s.entities.find((e) => e.id === bottom)!;
    const r = s.entities.find((e) => e.id === right)!;
    if (b.kind !== 'line' || r.kind !== 'line') throw new Error('lines expected');
    addConstraint(s, { kind: 'distance', a: b.a, b: b.b, axis: 'aligned', value: 100, at: [50, -10] });
    addConstraint(s, { kind: 'distance', a: r.a, b: r.b, axis: 'aligned', value: 60, at: [110, 30] });
    const solved = solve(s);
    expect(solved.ok).toBe(true);
    const p = (id: number) => point(solved.sketch, id);
    expect(dist(p(b.a), p(b.b))).toBeCloseTo(100, 6);
    expect(dist(p(r.a), p(r.b))).toBeCloseTo(60, 6);
    expect(p(b.a).y).toBeCloseTo(p(b.b).y, 6);
    expect(p(r.a).x).toBeCloseTo(p(r.b).x, 6);
  });

  it('refuses a dimension that contradicts the others', () => {
    const s = rectangle(40, 20);
    const [bottom, , top] = lineIds(s) as [number, number, number];
    const b = s.entities.find((e) => e.id === bottom)!;
    const t = s.entities.find((e) => e.id === top)!;
    if (b.kind !== 'line' || t.kind !== 'line') throw new Error('lines expected');
    addConstraint(s, { kind: 'distance', a: b.a, b: b.b, axis: 'aligned', value: 100, at: [0, 0] });
    addConstraint(s, { kind: 'distance', a: t.a, b: t.b, axis: 'aligned', value: 50, at: [0, 0] });
    expect(solve(s).ok).toBe(false);
  });

  it('makes a line tangent to a circle and points symmetric across a line', () => {
    const s = emptySketch();
    const c = addPoint(s, 0, 0);
    const circle = addEntity(s, { kind: 'circle', c, r: 10 });
    const a = addPoint(s, -20, 15), b = addPoint(s, 20, 13);
    const line = addEntity(s, { kind: 'line', a, b });
    addConstraint(s, { kind: 'fix', point: c });
    addConstraint(s, { kind: 'tangent', a: line, b: circle });
    const solved = solve(s);
    expect(solved.ok).toBe(true);
    const pa = point(solved.sketch, a), pb = point(solved.sketch, b);
    // Distance from the centre to the line is the radius, whichever gave way.
    const d = Math.abs((pb.x - pa.x) * pa.y - (pb.y - pa.y) * pa.x) / dist(pa, pb);
    const solvedCircle = solved.sketch.entities.find((e) => e.id === circle)!;
    expect(d).toBeCloseTo(solvedCircle.kind === 'circle' ? solvedCircle.r : -1, 6);

    const m1 = addPoint(s, 0, -50), m2 = addPoint(s, 0, 50);
    const axis = addEntity(s, { kind: 'line', a: m1, b: m2 });
    addConstraint(s, { kind: 'vertical', line: axis });
    const p = addPoint(s, -30, 5), q = addPoint(s, 28, 9);
    addEntity(s, { kind: 'line', a: p, b: q });
    addConstraint(s, { kind: 'symmetric', a: p, b: q, line: axis });
    const mirrored = solve(s);
    expect(mirrored.ok).toBe(true);
    const pp = point(mirrored.sketch, p), qq = point(mirrored.sketch, q), ax = point(mirrored.sketch, m1);
    expect(pp.x + qq.x).toBeCloseTo(2 * ax.x, 6);
    expect(pp.y).toBeCloseTo(qq.y, 6);
  });

  it('holds an angle between lines and a circle diameter', () => {
    const s = emptySketch();
    const o = addPoint(s, 0, 0), x = addPoint(s, 50, 0), y = addPoint(s, 30, 40);
    const l1 = addEntity(s, { kind: 'line', a: o, b: x });
    const l2 = addEntity(s, { kind: 'line', a: o, b: y });
    addConstraint(s, { kind: 'fix', point: o });
    addConstraint(s, { kind: 'horizontal', line: l1 });
    addConstraint(s, { kind: 'angle', a: l1, b: l2, value: 30, at: [20, 5] });
    const c = addPoint(s, 100, 0);
    const circle = addEntity(s, { kind: 'circle', c, r: 7 });
    addConstraint(s, { kind: 'diameter', curve: circle, value: 25, at: [110, 10] });
    const solved = solve(s);
    expect(solved.ok).toBe(true);
    const deg = (angleOf(point(solved.sketch, o), point(solved.sketch, y)) * 180) / Math.PI;
    expect(deg).toBeCloseTo(30, 5);
    const e = solved.sketch.entities.find((x) => x.id === circle)!;
    expect(e.kind === 'circle' && e.r).toBeCloseTo(12.5, 6);
  });

  it('pulls a dragged corner while the rectangle stays a rectangle', () => {
    const s = rectangle(40, 20);
    const corner = s.points.find((p) => p.x === 40 && p.y === 20)!;
    const dragged = solve(s, [{ point: corner.id, x: 55, y: 35 }]);
    expect(dragged.ok).toBe(true);
    const lines = dragged.sketch.entities.filter((e) => e.kind === 'line');
    for (const l of lines) {
      if (l.kind !== 'line') continue;
      const a = point(dragged.sketch, l.a), b = point(dragged.sketch, l.b);
      expect(Math.abs(a.x - b.x) < 1e-6 || Math.abs(a.y - b.y) < 1e-6).toBe(true);
    }
    expect(point(dragged.sketch, corner.id).x).toBeCloseTo(55, 3);
  });

  it('tells defined geometry from geometry that can still move', () => {
    const s = rectangle(40, 20);
    const ids = lineIds(s);
    const b = s.entities.find((e) => e.id === ids[0])!;
    const r = s.entities.find((e) => e.id === ids[1])!;
    if (b.kind !== 'line' || r.kind !== 'line') throw new Error('lines expected');
    const before = freedom(s)!;
    expect(s.entities.every((e) => !defined(e, before))).toBe(true);
    addConstraint(s, { kind: 'fix', point: b.a });
    addConstraint(s, { kind: 'distance', a: b.a, b: b.b, axis: 'aligned', value: 40, at: [0, 0] });
    addConstraint(s, { kind: 'distance', a: r.a, b: r.b, axis: 'aligned', value: 20, at: [0, 0] });
    const after = freedom(s)!;
    expect(s.entities.every((e) => defined(e, after))).toBe(true);
  });

  it('keeps a fillet tangent without counting it twice, and finds it fully defined once placed', async () => {
    const { fillet } = await import('./modify');
    const s = rectangle(100, 60);
    const ids = lineIds(s);
    const b = s.entities.find((e) => e.id === ids[0])!;
    const left = s.entities.find((e) => e.id === ids[3])!;
    if (b.kind !== 'line' || left.kind !== 'line') throw new Error('lines expected');
    addConstraint(s, { kind: 'distance', a: b.a, b: b.b, axis: 'horizontal', value: 120, at: [50, -10] });
    const first = solve(s);
    const corner = first.sketch.points.find((p) => p.x > 100 && p.y > 50)!;
    const f = fillet(first.sketch, corner.id, 10);
    if ('error' in f) throw new Error(f.error);
    const rounded = solve(f.sketch);
    expect(rounded.ok).toBe(true);
    expect(rounded.redundant).toEqual([]);
    expect(rounded.dof).toBe(3);
    const loose = freedom(f.sketch)!;
    expect(f.sketch.entities.every((e) => !defined(e, loose))).toBe(true);
    // Fix the lower left corner and dimension the height: nothing can move.
    addConstraint(f.sketch, { kind: 'fix', point: b.a });
    addConstraint(f.sketch, { kind: 'distance', a: left.a, b: left.b, axis: 'vertical', value: 60, at: [-10, 30] });
    const placed = solve(f.sketch);
    expect(placed.ok).toBe(true);
    expect(placed.dof).toBe(0);
    const none = freedom(f.sketch)!;
    expect(f.sketch.entities.every((e) => defined(e, none))).toBe(true);
  });
});
