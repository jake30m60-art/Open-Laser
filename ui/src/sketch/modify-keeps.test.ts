import { beforeAll, expect, it } from 'vitest';
import { emptySketch, type Sketch } from './model';
import { createTool, type ToolId } from './tools';
import { chamfer, extend, fillet } from './modify';
import { initSolver, looseAfter, solve } from './solver';
import { snap } from './snap';
beforeAll(async () => { await initSolver(); });

function drawn(s: Sketch, id: ToolId, clicks: [number, number][]): Sketch {
  const t = createTool(id);
  for (const [x, y] of clicks) { const sn = snap(s, { x, y }, { reach: 0.5, grid: 1, anchor: t.anchor() }); t.move(sn); t.click(s, t.resolve(sn, {}), {}); }
  return solve(s).sketch;
}
/** Points that were there before and moved when the edit was solved. */
function moved(before: Sketch, edited: Sketch): number[] {
  const after = solve(edited, [], new Map(), looseAfter(before, edited)).sketch;
  return before.points.filter((p) => {
    const q = after.points.find((x) => x.id === p.id);
    return q && Math.hypot(p.x - q.x, p.y - q.y) > 0.01;
  }).map((p) => p.id);
}

it('rounding or cutting a polygon corner leaves the other sides alone', () => {
  // The polygon's sides are equal; kept on the trimmed sides, that pulled the rest after them.
  const s = drawn(emptySketch(), 'polygon', [[0, 0], [30, 0]]);
  const corner = s.points.find((p) => s.entities.filter((e) => e.kind === 'line' && !e.construction && (e.a === p.id || e.b === p.id)).length === 2)!;
  for (const edit of [fillet, chamfer]) {
    const o = edit(s, corner.id, 5);
    if ('error' in o) throw new Error(o.error);
    expect(moved(s, o.sketch)).toEqual([]);
  }
});

it('extending a polygon side to a line leaves the other sides alone', () => {
  const s = drawn(emptySketch(), 'polygon', [[0, 0], [30, 0]]);
  const s2 = drawn(s, 'line', [[60, -60], [60, 60]]);
  const side = s2.entities.find((e) => e.kind === 'line' && !e.construction && extend(s2, e.id, { x: 50, y: 0 }) && !('error' in extend(s2, e.id, { x: 50, y: 0 })))!;
  const o = extend(s2, side.id, { x: 50, y: 0 });
  if ('error' in o) throw new Error(o.error);
  expect(moved(s2, o.sketch)).toEqual([]);
});
