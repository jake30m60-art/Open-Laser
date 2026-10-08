import { beforeAll, expect, it } from 'vitest';
import { addConstraint, addEntity, addPoint, clone, emptySketch, point, type Sketch } from './model';
import { createTool, type ToolId } from './tools';
import { fillet } from './modify';
import { initSolver, solveEdit } from './solver';
import { primaryRelations } from './primary-relations';

beforeAll(async () => { await initSolver(); });

/** How each pair of lines meeting at a point turns: the sign of their cross product. */
function turns(s: Sketch): Map<string, number> {
  const lines = s.entities.filter((e) => e.kind === 'line' && !e.construction) as Extract<Sketch['entities'][number], { kind: 'line' }>[];
  const out = new Map<string, number>();
  for (const a of lines) for (const b of lines) {
    if (a.id >= b.id) continue;
    const shared = [a.a, a.b].find((p) => p === b.a || p === b.b);
    if (shared === undefined) continue;
    const dir = (l: typeof a) => { const from = point(s, shared), to = point(s, l.a === shared ? l.b : l.a); return { x: to.x - from.x, y: to.y - from.y }; };
    const u = dir(a), v = dir(b);
    const cross = u.x * v.y - u.y * v.x, len = Math.hypot(u.x, u.y) * Math.hypot(v.x, v.y);
    if (len > 1e-9 && Math.abs(cross) / len > 0.02) out.set(`${a.id}/${b.id}`, Math.sign(cross));
  }
  return out;
}

function shape(tool: ToolId, clicks: [number, number][], fillets = 0): Sketch {
  const s = emptySketch(), t = createTool(tool);
  for (const [x, y] of clicks) t.click(s, { x, y, kind: 'free' }, {});
  let current = s;
  for (const p of s.points.slice(0, fillets)) {
    const f = fillet(current, p.id, 3); if ('error' in f) throw Error(f.error);
    const out = solveEdit(current, f.sketch); if (!out.ok) throw Error('fillet');
    current = out.sketch;
  }
  return current;
}

/** Dimensions an operator would put on: each line's length, and the angle at each corner. */
function dimensioned(s: Sketch): Sketch[] {
  const lines = s.entities.filter((e) => e.kind === 'line' && !e.construction) as Extract<Sketch['entities'][number], { kind: 'line' }>[];
  const out: Sketch[] = [];
  for (const l of lines) {
    const n = clone(s);
    addConstraint(n, { kind: 'distance', a: l.a, b: l.b, axis: 'aligned', value: Math.hypot(point(s, l.b).x - point(s, l.a).x, point(s, l.b).y - point(s, l.a).y), at: [0, 0] });
    out.push(n);
  }
  return out;
}

it('no dimension change mirrors part of a shape', () => {
  const failures: string[] = [];
  const cases: [string, Sketch][] = [
    ['rect', shape('rect2', [[0, 0], [100, 60]])],
    ['rect filleted', shape('rect2', [[0, 0], [100, 60]], 4)],
    ['centre rect', shape('rectc', [[50, 30], [100, 60]])],
    ['hexagon', shape('polygon', [[0, 0], [40, 10]])],
    ['triangle', shape('polygon', [[0, 0], [30, 5]].map((p) => p as [number, number]))],
    ['slot', shape('slot', [[0, 0], [60, 10], [30, 20]])],
  ];
  const tri = emptySketch();
  { const a = addPoint(tri, 0, 0), b = addPoint(tri, 60, 0), c = addPoint(tri, 15, 40);
    addEntity(tri, { kind: 'line', a, b }); addEntity(tri, { kind: 'line', a: b, b: c }); addEntity(tri, { kind: 'line', a: c, b: a }); }
  cases.push(['free triangle', tri]);
  // A hexagon as older builds drew it: equal sides, no circle around it.
  const legacy = emptySketch();
  { const ids = Array.from({ length: 6 }, (_, i) => addPoint(legacy, 40 * Math.cos(i * Math.PI / 3), 40 * Math.sin(i * Math.PI / 3)));
    const lines = ids.map((id, i) => addEntity(legacy, { kind: 'line', a: id, b: ids[(i + 1) % 6]! }));
    for (const l of lines.slice(1)) addConstraint(legacy, { kind: 'equal', a: lines[0]!, b: l });
    // Older builds also made both level sides horizontal.
    addConstraint(legacy, { kind: 'horizontal', line: lines[1]! }); addConstraint(legacy, { kind: 'horizontal', line: lines[4]! }); }
  // Opening a sketch upgrades it, as the store does.
  cases.push(['legacy hexagon', primaryRelations(legacy)]);
  for (const [name, base] of cases) {
    for (const withDim of dimensioned(base)) {
      const start = solveEdit(base, withDim);
      if (!start.ok) continue;
      const dim = start.sketch.constraints.at(-1)!;
      if (dim.kind !== 'distance') continue;
      for (const factor of [0.25, 0.5, 2, 4]) {
        const next = clone(start.sketch);
        const edit = next.constraints.find((c) => c.id === dim.id)!;
        if (edit.kind !== 'distance') continue;
        edit.value = dim.value * factor;
        const out = solveEdit(start.sketch, next);
        if (!out.ok) continue;
        const before = turns(start.sketch), after = turns(out.sketch);
        for (const [pair, sign] of before) if (after.has(pair) && after.get(pair) !== sign) failures.push(`${name}: length ×${factor} flips ${pair}`);
      }
    }
  }
  expect(failures).toEqual([]);
});

it('opening or closing a corner angle a lot never mirrors the shape', () => {
  const failures: string[] = [];
  for (const [name, base] of [['rect', shape('rect2', [[0, 0], [100, 60]])], ['slot', shape('slot', [[0, 0], [60, 10], [30, 20]])]] as const) {
    // A line hanging off a corner, dimensioned by its angle to the side.
    const s = clone(base);
    const corner = s.points[2]!.id, tip = addPoint(s, point(s, corner).x + 30, point(s, corner).y + 10);
    const arm = addEntity(s, { kind: 'line', a: corner, b: tip });
    const side = s.entities.find((e) => e.kind === 'line' && (e.a === corner || e.b === corner) && e.id !== arm)!;
    const withAngle = clone(s);
    addConstraint(withAngle, { kind: 'angle', a: side.id, b: arm, value: 20, at: [0, 0] });
    const start = solveEdit(s, withAngle);
    if (!start.ok) continue;
    for (const target of [60, 100, 150, 170, 5]) {
      const next = clone(start.sketch);
      const angle = next.constraints.at(-1)!;
      if (angle.kind !== 'angle') continue;
      angle.value = target;
      const out = solveEdit(start.sketch, next);
      if (!out.ok) continue;
      const moved = base.points.some((p) => Math.hypot(point(out.sketch, p.id).x - point(start.sketch, p.id).x, point(out.sketch, p.id).y - point(start.sketch, p.id).y) > 1e-4);
      if (moved) failures.push(`${name}: angle ${target} moves the shape`);
    }
  }
  expect(failures).toEqual([]);
});
