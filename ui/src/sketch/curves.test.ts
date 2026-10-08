import { beforeAll, describe, expect, it } from 'vitest';
import { addConstraint, addEntity, addPoint, clampedKnots, emptySketch, point, type Sketch } from './model';
import { createTool } from './tools';
import { initSolver, solve } from './solver';
import { arcParams, ellipseFrame, onEllipse, sampled, splineAt } from './curves';
import { boundsOf, distanceTo, intersections, shapeOf } from './geometry';
import { mirror } from './modify';
import { toDxf } from './dxf';
import type { Snap } from './snap';

beforeAll(async () => { await initSolver(); });

const free = (x: number, y: number): Snap => ({ x, y, kind: 'free' });

function ellipse(): Sketch {
  const s = emptySketch();
  const tool = createTool('ellipse');
  tool.click(s, free(0, 0), {});
  tool.click(s, free(30, 0), {});
  tool.click(s, free(5, 10), {});
  return s;
}

describe('ellipses', () => {
  it('draws centre, major axis and minor radius, and runs where it should', () => {
    const s = ellipse();
    const e = s.entities[0]!;
    if (e.kind !== 'ellipse') throw new Error('ellipse expected');
    expect(e.r).toBeCloseTo(10);
    const f = ellipseFrame(s, e);
    expect(onEllipse(f, 0)).toEqual({ x: 30, y: 0 });
    expect(onEllipse(f, Math.PI / 2).y).toBeCloseTo(10);
    expect(boundsOf(shapeOf(s, e))).toMatchObject({ minX: -30, maxX: 30 });
    expect(distanceTo(shapeOf(s, e), { x: 0, y: 12 })).toBeCloseTo(2, 1);
  });

  it('turns its axes when drawn wider than long', () => {
    const s = emptySketch();
    const tool = createTool('ellipse');
    tool.click(s, free(0, 0), {});
    tool.click(s, free(5, 0), {});
    tool.click(s, free(0, 20), {});
    const e = s.entities[0]!;
    if (e.kind !== 'ellipse') throw new Error('ellipse expected');
    expect(e.r).toBeCloseTo(5);
    expect(point(s, e.m).y).toBeCloseTo(20);
  });

  it('keeps its size through the solver and takes a minor radius dimension', () => {
    const s = ellipse();
    const e = s.entities[0]!;
    if (e.kind !== 'ellipse') throw new Error('ellipse expected');
    addConstraint(s, { kind: 'fix', point: e.c });
    addConstraint(s, { kind: 'distance', a: e.c, b: e.m, axis: 'aligned', value: 40, at: [0, 0] });
    addConstraint(s, { kind: 'radius', curve: e.id, value: 15, at: [0, 0] });
    const out = solve(s);
    expect(out.ok).toBe(true);
    const solved = out.sketch.entities[0]!;
    if (solved.kind !== 'ellipse') throw new Error('ellipse expected');
    expect(solved.r).toBeCloseTo(15, 4);
    expect(Math.hypot(point(out.sketch, solved.m).x, point(out.sketch, solved.m).y)).toBeCloseTo(40, 4);
  });

  it('as an arc, solves with its ends on it', () => {
    const s = emptySketch();
    const c = addPoint(s, 0, 0), m = addPoint(s, 20, 0);
    const a = addPoint(s, 20, 0.0001), b = addPoint(s, -20, 0);
    addEntity(s, { kind: 'ellipse-arc', c, m, r: 10, a, b });
    const out = solve(s);
    expect(out.ok).toBe(true);
    const arc = out.sketch.entities[0]!;
    if (arc.kind !== 'ellipse-arc') throw new Error('arc expected');
    const [from, to] = arcParams(out.sketch, arc);
    expect(to - from).toBeCloseTo(Math.PI, 2);
  });

  it('exports as an ELLIPSE the importer reads', () => {
    const dxf = toDxf(ellipse());
    expect(dxf).toContain('\r\nELLIPSE\r\n');
    expect(dxf).toMatch(/\r\n40\r\n0\.333333333333\r\n41\r\n0\r\n42\r\n6\.28318530718\r\n/);
  });
});

describe('splines', () => {
  function spline(): Sketch {
    const s = emptySketch();
    const tool = createTool('spline');
    for (const [x, y] of [[0, 0], [10, 10], [20, -10], [30, 0]] as const) tool.click(s, free(x, y), {});
    return s;
  }

  it('grows a pole per click, cubic from four, through its ends', () => {
    const s = spline();
    const e = s.entities.find((x) => x.kind === 'spline');
    if (!e || e.kind !== 'spline') throw new Error('spline expected');
    expect(e.poles).toHaveLength(4);
    expect(e.degree).toBe(3);
    expect(e.knots).toEqual(clampedKnots(4, 3));
    const pts = sampled(s, e);
    expect(pts[0]).toEqual({ x: 0, y: 0 });
    expect(pts.at(-1)!.x).toBeCloseTo(30);
    expect(s.entities.filter((x) => x.kind === 'spline')).toHaveLength(1);
  });

  it('closes on its first pole', () => {
    const s = emptySketch();
    const tool = createTool('spline');
    for (const [x, y] of [[0, 0], [10, 0], [10, 10]] as const) tool.click(s, free(x, y), {});
    const first = s.entities[0]!.kind === 'spline' ? (s.entities[0] as { poles: number[] }).poles[0]! : -1;
    expect(tool.click(s, { x: 0, y: 0, kind: 'end', point: first }, {})).toBe('done');
    const e = s.entities[0]!;
    if (e.kind !== 'spline') throw new Error('spline expected');
    expect(e.poles.at(-1)).toBe(e.poles[0]);
  });

  it('evaluates a rational quadratic as an exact quarter circle', () => {
    const w = Math.SQRT1_2;
    const poles = [{ x: 10, y: 0 }, { x: 10, y: 10 }, { x: 0, y: 10 }];
    for (let i = 0; i <= 20; i++) {
      const p = splineAt(poles, 2, [0, 0, 0, 1, 1, 1], [1, w, 1], i / 20);
      expect(Math.hypot(p.x, p.y)).toBeCloseTo(10, 9);
    }
  });

  it('is hit, crossed, mirrored and exported', () => {
    const s = spline();
    const e = s.entities[0]!;
    const line = addEntity(s, { kind: 'line', a: addPoint(s, 15, -20), b: addPoint(s, 15, 20) });
    expect(intersections(shapeOf(s, e), shapeOf(s, s.entities.find((x) => x.id === line)!)).length).toBeGreaterThan(0);
    const mirrored = mirror(s, [e.id], line);
    if ('error' in mirrored) throw new Error(mirrored.error);
    expect(mirrored.sketch.entities.filter((x) => x.kind === 'spline')).toHaveLength(2);
    const dxf = toDxf(s);
    expect(dxf).toContain('\r\nSPLINE\r\n');
    expect(dxf).toContain('\r\n71\r\n3\r\n72\r\n8\r\n73\r\n4\r\n');
  });
});
