import { beforeAll, expect, it } from 'vitest';
import { addConstraint, clone, emptySketch, type Sketch } from './model';
import { createTool } from './tools';
import { initSolver, solveEdit } from './solver';
import { snap } from './snap';

beforeAll(async () => { await initSolver(); });

/** Draws with typed field values, as the heads-up fields give them. */
function draw(s: Sketch, id: 'rect2' | 'circle', clicks: [number, number][], typed: Record<string, number>): void {
  const tool = createTool(id);
  clicks.forEach(([x, y], i) => {
    const sn = snap(s, { x, y }, { reach: 0.01, grid: 0, anchor: tool.anchor() });
    tool.move(sn);
    const locked = i === clicks.length - 1 ? typed : {};
    tool.click(s, tool.resolve(sn, locked), locked);
  });
}

it('a typed rectangle keeps its size and moves whole when dimensioned to an earlier circle', () => {
  const s = emptySketch();
  // As drawn: the circle first, then the rectangle beside it.
  draw(s, 'circle', [[100, 25], [110, 25]], { diameter: 20 });
  draw(s, 'rect2', [[0, 0], [50, 50]], { width: 50, height: 50 });
  const circle = s.entities.find((e) => e.kind === 'circle')!;
  const centre = circle.kind === 'circle' ? circle.c : -1;
  const corner = s.points.find((p) => Math.abs(p.x) < 1e-9 && Math.abs(p.y) < 1e-9)!.id;
  // The circle's centre 25 mm right of the rectangle's left edge: centred.
  const next = clone(s);
  addConstraint(next, { kind: 'distance', a: corner, b: centre, axis: 'horizontal', value: 25, at: [0, -10] });
  const solved = solveEdit(s, next);
  expect(solved.ok).toBe(true);
  const xs = solved.sketch.points.filter((p) => p.id !== centre).map((p) => p.x);
  const width = Math.max(...xs) - Math.min(...xs);
  expect(width).toBeCloseTo(50, 6);
  const c = solved.sketch.points.find((p) => p.id === centre)!;
  expect(c.x).toBeCloseTo(100, 6);
  expect(Math.min(...xs)).toBeCloseTo(75, 6);
});

it('two edges symmetric about a circle centre the rectangle on it, keeping its size', () => {
  const s = emptySketch();
  draw(s, 'circle', [[100, 25], [110, 25]], { diameter: 20 });
  draw(s, 'rect2', [[0, 0], [50, 50]], { width: 50, height: 50 });
  const circle = s.entities.find((e) => e.kind === 'circle')!;
  const at = (x: number, y: number) => s.points.find((p) => Math.abs(p.x - x) < 1e-9 && Math.abs(p.y - y) < 1e-9)!.id;
  // The left and right edges: one corner across the centre from its mirror.
  const next = clone(s);
  addConstraint(next, { kind: 'symmetric', a: at(0, 0), b: at(50, 50), line: circle.id });
  const solved = solveEdit(s, next);
  expect(solved.ok).toBe(true);
  const c = circle.kind === 'circle' ? circle.c : -1;
  const rect = solved.sketch.points.filter((p) => p.id !== c);
  const xs = rect.map((p) => p.x), ys = rect.map((p) => p.y);
  expect(Math.max(...xs) - Math.min(...xs)).toBeCloseTo(50, 6);
  expect((Math.max(...xs) + Math.min(...xs)) / 2).toBeCloseTo(100, 6);
  expect((Math.max(...ys) + Math.min(...ys)) / 2).toBeCloseTo(25, 6);
});
