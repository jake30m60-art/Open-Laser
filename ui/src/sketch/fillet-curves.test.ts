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

import { chamfer } from './modify';

it('chamfers a corner where a line meets an arc, the same distance back along each', () => {
  const { s, top } = slot();
  const out = chamfer(s, top, 4);
  if ('error' in out) throw new Error(out.error);
  const solved = solve(out.sketch, [], new Map());
  expect(solved.ok).toBe(true);
  const cut = solved.sketch.entities.find((e) => e.id === out.created[0])!;
  if (cut.kind !== 'line') throw new Error('not a line');
  const [p, q] = [point(solved.sketch, cut.a), point(solved.sketch, cut.b)];
  const onLine = [p, q].find((x) => Math.abs(x.y - 20) < 1e-6 && x.x > 1)!;
  const onArc = [p, q].find((x) => x !== onLine)!;
  expect(onLine.x).toBeCloseTo(4, 3);
  expect(Math.hypot(onArc.x, onArc.y)).toBeCloseTo(20, 3);
  // 4 mm along the arc from the top: an angle of 4/20 rad.
  expect(Math.atan2(onArc.x, onArc.y)).toBeCloseTo(4 / 20, 3);
});

/** Two quarter arcs meeting at a pointed tip, as an ogive. */
function tip() {
  const s = emptySketch();
  const t = addPoint(s, 0, 50);
  const c1 = addPoint(s, 30, 0), l = addPoint(s, -28.3, 0);
  const c2 = addPoint(s, -30, 0), rr = addPoint(s, 28.3, 0);
  // Left side: centre right of the axis, from the tip round to the left foot.
  addEntity(s, { kind: 'arc', c: c1, a: t, b: l });
  addEntity(s, { kind: 'arc', c: c2, a: rr, b: t });
  return { s, t };
}

it('rounds and chamfers where two arcs meet', () => {
  for (const tool of [fillet, chamfer]) {
    const { s, t } = tip();
    const out = tool(s, t, 3);
    if ('error' in out) throw new Error(out.error);
    expect(solve(out.sketch, [], new Map()).ok).toBe(true);
  }
});
