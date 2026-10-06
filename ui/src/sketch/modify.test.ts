import { beforeAll, describe, expect, it } from 'vitest';
import { addEntity, addPoint, emptySketch, point, type Id, type Sketch } from './model';
import { createTool } from './tools';
import { breakAt, chamfer, circularPattern, extend, fillet, mirror, offset, rectangularPattern, trim, type Outcome } from './modify';
import { initSolver, solve } from './solver';
import { boundsOf, dist, shapeOf, sketchBounds, sweep, unionBounds } from './geometry';
import type { Snap } from './snap';

beforeAll(async () => { await initSolver(); });

const free = (x: number, y: number): Snap => ({ x, y, kind: 'free' });

function ok(o: Outcome): Sketch {
  if ('error' in o) throw new Error(o.error);
  return o.sketch;
}

function line(s: Sketch, x1: number, y1: number, x2: number, y2: number): Id {
  return addEntity(s, { kind: 'line', a: addPoint(s, x1, y1), b: addPoint(s, x2, y2) });
}

function rectangle(w: number, h: number): Sketch {
  const s = emptySketch();
  const tool = createTool('rect2');
  tool.click(s, free(0, 0), {});
  tool.click(s, free(w, h), {});
  return s;
}

const length = (s: Sketch, id: Id) => {
  const e = s.entities.find((x) => x.id === id)!;
  return e.kind === 'line' ? dist(point(s, e.a), point(s, e.b)) : NaN;
};

describe('trim, break and extend', () => {
  it('trims the arm of a cross back to the crossing', () => {
    const s = emptySketch();
    const h = line(s, -10, 0, 10, 0);
    line(s, 0, -10, 0, 10);
    const t = ok(trim(s, h, { x: 7, y: 0 }));
    expect(length(t, h)).toBeCloseTo(10);
    expect(t.constraints.some((c) => c.kind === 'on')).toBe(true);
    expect(solve(t).ok).toBe(true);
  });

  it('cuts the stretch between two crossings out of a line', () => {
    const s = emptySketch();
    const h = line(s, 0, 0, 30, 0);
    line(s, 10, -5, 10, 5);
    line(s, 20, -5, 20, 5);
    const t = ok(trim(s, h, { x: 15, y: 0 }));
    const lines = t.entities.filter((e) => e.kind === 'line' && Math.abs(point(t, e.a).y) < 1e-9 && Math.abs(point(t, e.b).y) < 1e-9);
    expect(lines).toHaveLength(2);
    expect(lines.map((l) => length(t, l.id)).sort()).toEqual([10, 10]);
  });

  it('turns a circle into the arc the click did not touch', () => {
    const s = emptySketch();
    const c = addEntity(s, { kind: 'circle', c: addPoint(s, 0, 0), r: 10 });
    line(s, -20, 0, 20, 0);
    const t = ok(trim(s, c, { x: 0, y: 10 }));
    const arc = t.entities.find((e) => e.id === c)!;
    expect(arc.kind).toBe('arc');
    const shape = shapeOf(t, arc);
    if (shape.kind !== 'arc') throw new Error('arc expected');
    expect(sweep(shape.start, shape.end)).toBeCloseTo(Math.PI);
    expect(boundsOf(shape).maxY).toBeLessThan(1e-9);
  });

  it('breaks a line into the pieces between its crossings', () => {
    const s = emptySketch();
    const h = line(s, 0, 0, 30, 0);
    line(s, 10, -5, 10, 5);
    line(s, 20, -5, 20, 5);
    const b = breakAt(s, h, { x: 15, y: 0 });
    if ('error' in b) throw new Error(b.error);
    expect(b.created).toHaveLength(3);
    expect(b.created.map((id) => length(b.sketch, id))).toEqual([10, 10, 10]);
  });

  it('extends a line to the next curve ahead of the clicked end', () => {
    const s = emptySketch();
    const h = line(s, 0, 0, 10, 0);
    line(s, 25, -5, 25, 5);
    const t = ok(extend(s, h, { x: 9, y: 0 }));
    expect(length(t, h)).toBeCloseTo(25);
    expect(solve(t).ok).toBe(true);
  });
});

describe('fillet and chamfer', () => {
  it('rounds a rectangle corner with a tangent arc of the radius', () => {
    const s = rectangle(40, 20);
    const corner = s.points.find((p) => p.x === 40 && p.y === 20)!;
    const f = fillet(s, corner.id, 5);
    if ('error' in f) throw new Error(f.error);
    const arc = f.sketch.entities.find((e) => e.id === f.created[0])!;
    const shape = shapeOf(f.sketch, arc);
    expect(shape.kind === 'arc' && shape.r).toBeCloseTo(5);
    expect(shape.kind === 'arc' && sweep(shape.start, shape.end)).toBeCloseTo(Math.PI / 2);
    const solved = solve(f.sketch);
    expect(solved.ok).toBe(true);
    const b = sketchBounds(solved.sketch)!;
    expect(b.maxX).toBeCloseTo(40);
    expect(b.maxY).toBeCloseTo(20);
  });

  it('refuses a fillet larger than the lines allow', () => {
    const s = rectangle(10, 10);
    const corner = s.points.find((p) => p.x === 10 && p.y === 10)!;
    expect('error' in fillet(s, corner.id, 20)).toBe(true);
  });

  it('chamfers a corner the distance back along each side', () => {
    const s = rectangle(40, 20);
    const corner = s.points.find((p) => p.x === 0 && p.y === 0)!;
    const c = chamfer(s, corner.id, 3);
    if ('error' in c) throw new Error(c.error);
    expect(length(c.sketch, c.created[0]!)).toBeCloseTo(3 * Math.SQRT2);
    expect(c.sketch.entities.filter((e) => e.kind === 'line')).toHaveLength(5);
  });
});

describe('offset, mirror and patterns', () => {
  it('offsets a closed rectangle outward with its corners kept', () => {
    const s = rectangle(40, 20);
    const ids = s.entities.map((e) => e.id);
    const o = offset(s, ids, 2, { x: -5, y: 10 });
    if ('error' in o) throw new Error(o.error);
    expect(o.created).toHaveLength(4);
    const b = unionBounds(o.created.map((id) => boundsOf(shapeOf(o.sketch, o.sketch.entities.find((e) => e.id === id)!))))!;
    expect([b.minX, b.minY, b.maxX, b.maxY].map((v) => Math.round(v * 1e6) / 1e6)).toEqual([-2, -2, 42, 22]);
    // The new outline is closed: four corners, each shared by two lines.
    const newPoints = new Set(o.created.flatMap((id) => { const e = o.sketch.entities.find((x) => x.id === id)!; return e.kind === 'line' ? [e.a, e.b] : []; }));
    expect(newPoints.size).toBe(4);
  });

  it('offsets inward when the side picked is inside, and grows a circle outward', () => {
    const s = rectangle(40, 20);
    const ids = s.entities.map((e) => e.id);
    const o = offset(s, ids, 2, { x: 20, y: 10 });
    if ('error' in o) throw new Error(o.error);
    const b = unionBounds(o.created.map((id) => boundsOf(shapeOf(o.sketch, o.sketch.entities.find((e) => e.id === id)!))))!;
    expect(b.minX).toBeCloseTo(2);
    expect(b.maxY).toBeCloseTo(18);
    const c = emptySketch();
    const circle = addEntity(c, { kind: 'circle', c: addPoint(c, 0, 0), r: 5 });
    const grown = offset(c, [circle], 3, { x: 20, y: 0 });
    if ('error' in grown) throw new Error(grown.error);
    const made = grown.sketch.entities.find((e) => e.id === grown.created[0])!;
    expect(made.kind === 'circle' && made.r).toBeCloseTo(8);
  });

  it('mirrors across a line and ties each copy to its original', () => {
    const s = emptySketch();
    const axis = line(s, 0, -50, 0, 50);
    const l = line(s, -30, 5, -10, 20);
    const m = mirror(s, [l], axis);
    if ('error' in m) throw new Error(m.error);
    const made = m.sketch.entities.find((e) => e.id === m.created[0])!;
    if (made.kind !== 'line') throw new Error('line expected');
    expect(point(m.sketch, made.a).x).toBeCloseTo(30);
    expect(point(m.sketch, made.b).y).toBeCloseTo(20);
    expect(m.sketch.constraints.filter((c) => c.kind === 'symmetric')).toHaveLength(2);
    expect(solve(m.sketch).ok).toBe(true);
  });

  it('patterns holes in a grid and round a centre', () => {
    const s = emptySketch();
    const hole = addEntity(s, { kind: 'circle', c: addPoint(s, 10, 10), r: 2 });
    const grid = rectangularPattern(s, [hole], 4, 3, 15, 12);
    if ('error' in grid) throw new Error(grid.error);
    expect(grid.created).toHaveLength(11);
    const b = sketchBounds(grid.sketch)!;
    expect(b.maxX).toBeCloseTo(10 + 3 * 15 + 2);
    expect(b.maxY).toBeCloseTo(10 + 2 * 12 + 2);
    const round = circularPattern(s, [hole], { x: 0, y: 0 }, 6, 360);
    if ('error' in round) throw new Error(round.error);
    expect(round.created).toHaveLength(5);
    for (const id of round.created) {
      const e = round.sketch.entities.find((x) => x.id === id)!;
      if (e.kind !== 'circle') throw new Error('circle expected');
      expect(dist(point(round.sketch, e.c), { x: 0, y: 0 })).toBeCloseTo(Math.hypot(10, 10));
    }
  });
});
