import { beforeAll, expect, it } from 'vitest';
import { addConstraint, emptySketch, type Sketch } from './model';
import { createTool, type ToolId } from './tools';
import { initSolver, solve } from './solver';
import { snap } from './snap';
beforeAll(async () => { await initSolver(); });
function drawn(id: ToolId, clicks: [number, number][]): Sketch {
  const s = emptySketch(); const t = createTool(id);
  for (const [x, y] of clicks) { const sn = snap(s, { x, y }, { reach: 0.5, grid: 1, anchor: t.anchor() }); t.move(sn); t.click(s, t.resolve(sn, {}), {}); }
  return s;
}
const cases: Array<[string, ToolId, [number, number][]]> = [
  ['line chain closed', 'line', [[0,0],[50,0],[50,30],[0,30],[0,0]]],
  ['rect2', 'rect2', [[0,0],[50,30]]],
  ['rectc', 'rectc', [[0,0],[25,15]]],
  ['circle', 'circle', [[0,0],[10,0]]],
  ['polygon', 'polygon', [[0,0],[20,0]]],
  ['slot', 'slot', [[0,0],[40,0],[20,5]]],
  ['arc3', 'arc3', [[0,0],[20,0],[10,8]]],
];
/** Every drawing tool records only relations that each say something new,
 *  so a dimension added afterwards is judged on its own. */
it('no drawing tool leaves a constraint the others already imply', () => {
  for (const [name, id, clicks] of cases) {
    const s = drawn(id, clicks);
    const base = solve(s);
    const e = s.entities.find((x) => !x.construction)!;
    if (e.kind === 'line') addConstraint(s, { kind: 'distance', a: e.a, b: e.b, axis: 'aligned', value: 40, at: [0, 0] });
    else addConstraint(s, { kind: e.kind === 'circle' ? 'diameter' : 'radius', curve: e.id, value: 12, at: [0, 0] });
    const after = solve(s);
    expect(base.redundant, name).toEqual([]);
    expect(after.ok, name).toBe(true);
    expect(after.redundant, name).toEqual([]);
    expect(after.dof, name).toBe(base.dof - 1);
  }
});
