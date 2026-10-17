import { beforeAll, expect, it } from 'vitest';
import { addEntity, addPoint, emptySketch, point, type Sketch } from './model';
import { createTool } from './tools';
import { fillet, mirror, type Outcome } from './modify';
import { snap } from './snap';
import { initSolver, solve } from './solver';
import type { Id } from './model';

beforeAll(async () => { await initSolver(); });

const ok = (o: Outcome): Sketch => { if ('error' in o) throw new Error(o.error); return o.sketch; };

/** Half an outline open at x = 0, and a vertical mirror line there. */
function half(): { s: Sketch; lines: Id[]; axis: Id } {
  const s = emptySketch();
  const p = [addPoint(s, 0, 0), addPoint(s, -40, 0), addPoint(s, -40, 30), addPoint(s, 0, 30)];
  const lines = [0, 1, 2].map((i) => addEntity(s, { kind: 'line', a: p[i]!, b: p[i + 1]! }));
  const axis = addEntity(s, { kind: 'line', a: addPoint(s, 0, -10), b: addPoint(s, 0, 50) });
  return { s, lines, axis };
}

it('rounds a corner whose two lines were mirrored one after the other', () => {
  const { s, lines, axis } = half();
  const once = ok(mirror(s, [lines[2]!], axis));
  const twice = ok(mirror(once, [lines[0]!, lines[1]!], axis));
  const corner = twice.points.find((p) => p.x === 40 && p.y === 30)!;
  expect(twice.points.filter((p) => p.x === 40 && p.y === 30)).toHaveLength(1);
  const rounded = ok(fillet(twice, corner.id, 5));
  expect(rounded.entities.some((e) => e.kind === 'arc')).toBe(true);
  expect(solve(rounded).ok).toBe(true);
});

it('rounds a corner left as two separate points by earlier mirrors', () => {
  const s = emptySketch();
  const v1 = addPoint(s, 40, 30), v2 = addPoint(s, 40, 30);
  addEntity(s, { kind: 'line', a: addPoint(s, 0, 30), b: v1 });
  addEntity(s, { kind: 'line', a: v2, b: addPoint(s, 40, 0) });
  const rounded = ok(fillet(s, v2, 5));
  expect(rounded.entities.filter((e) => e.kind === 'arc')).toHaveLength(1);
});

it('a line snapped to a spline control point by the grid shares it and follows the spline', () => {
  const s = emptySketch();
  const spline = createTool('spline');
  for (const [x, y] of [[0, 0], [20, 30], [40, 0]] as const) spline.click(s, { x, y, kind: 'free' }, {});
  const pole = s.points.find((p) => p.x === 20 && p.y === 30)!;
  // The cursor is beyond a point's reach; the grid puts it on the pole.
  const at = snap(s, { x: 21.4, y: 28.8 }, { reach: 1, grid: 5 });
  expect(at.point).toBe(pole.id);
  const line = createTool('line');
  line.click(s, at, {});
  line.click(s, { x: 20, y: 60, kind: 'free' }, {});
  const drawn = s.entities.find((e) => e.kind === 'line')!;
  expect(drawn.kind === 'line' && drawn.a).toBe(pole.id);
  const pulled = solve(s, [{ point: pole.id, x: 25, y: 40 }]);
  expect(point(pulled.sketch, drawn.kind === 'line' ? drawn.a : -1)).toMatchObject({ x: 25, y: 40 });
});
