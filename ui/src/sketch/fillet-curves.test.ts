import { beforeAll, expect, it } from 'vitest';
import { addEntity, addPoint, emptySketch, point, type Arc } from './model';
import { fillet } from './modify';
import { initSolver, solve } from './solver';
beforeAll(async () => { await initSolver(); });

/** A line running into a semicircle at its end, as a slot's side meets its round end. */
function slot() {
  const s = emptySketch();
  const c = addPoint(s, 0, 0), top = addPoint(s, 0, 20), bottom = addPoint(s, 0, -20);
  // The right half of a circle of radius 20, from the bottom round to the top.
  addEntity(s, { kind: 'arc', c, a: bottom, b: top });
  // A line leaving the top end straight to the right: a corner of 90°.
  const right = addPoint(s, 80, 20);
  addEntity(s, { kind: 'line', a: top, b: right });
  return { s, top };
}

it('rounds a corner where a line meets an arc, tangent to both', () => {
  const { s, top } = slot();
  const out = fillet(s, top, 5);
  if ('error' in out) throw new Error(out.error);
  const solved = solve(out.sketch, [], new Map());
  expect(solved.ok).toBe(true);
  const arc = solved.sketch.entities.find((e) => e.id === out.created[0]) as Arc;
  const c = point(solved.sketch, arc.c), a = point(solved.sketch, arc.a), b = point(solved.sketch, arc.b);
  expect(Math.hypot(a.x - c.x, a.y - c.y)).toBeCloseTo(5, 3);
  expect(Math.hypot(b.x - c.x, b.y - c.y)).toBeCloseTo(5, 3);
  // Tangent to the line y = 20: the centre is 5 off it.
  expect(Math.abs(c.y - 20)).toBeCloseTo(5, 3);
  // Tangent to the big arc: the centre is 20 ± 5 from its centre.
  const d = Math.hypot(c.x, c.y);
  expect(Math.min(Math.abs(d - 25), Math.abs(d - 15))).toBeLessThan(1e-3);
});
