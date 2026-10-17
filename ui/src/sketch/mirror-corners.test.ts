import { beforeAll, expect, it } from 'vitest';
import { emptySketch } from './model';
import { createTool } from './tools';
import { chamfer, fillet, mirror } from './modify';
import { snap } from './snap';
import { initSolver, solve, solveEdit } from './solver';
beforeAll(async () => { await initSolver(); });
/** A side drawn to a mirror line and mirrored: the corner between them rounds and chamfers
 *  however the mirror line was drawn, with or without automatic constraints. */
it('fillets and chamfers a side and its mirror image for every way of drawing the mirror line', () => {
  const out: string[] = [];
  for (const auto of [false, true]) for (const order of ['axisFirst', 'lineFirst', 'axisFromCorner'] as const) for (const apexY of [20, 23.7]) {
    const s = emptySketch();
    const drawAxis = () => { const t = createTool('line', auto); t.click(s, snap(s, { x: 0, y: -10 }, { reach: 1, grid: 0 }), {}); t.click(s, snap(s, { x: 0, y: 50 }, { reach: 1, grid: 0, anchor: { x: 0, y: -10 } }), {}); };
    const drawSide = () => { const t = createTool('line', auto); t.click(s, snap(s, { x: -30, y: 0 }, { reach: 1, grid: 0 }), {}); t.click(s, snap(s, { x: 0, y: apexY }, { reach: 1, grid: 0, anchor: { x: -30, y: 0 } }), {}); };
    const axisFromCorner = () => { const t = createTool('line', auto); t.click(s, snap(s, { x: 0, y: apexY }, { reach: 1, grid: 0 }), {}); t.click(s, snap(s, { x: 0, y: -10 }, { reach: 1, grid: 0, anchor: { x: 0, y: apexY } }), {}); };
    if (order === 'axisFirst') { drawAxis(); drawSide(); } else if (order === 'lineFirst') { drawSide(); drawAxis(); } else { drawSide(); axisFromCorner(); }
    const settled = solve(s).sketch;
    const axis = settled.entities.find((e) => e.kind === 'line' && [e.a, e.b].some((id) => settled.points.find((p) => p.id === id)!.y === -10))!;
    const side = settled.entities.find((e) => e.id !== axis.id)!;
    const m = mirror(settled, [side.id], axis.id); if ('error' in m) { out.push('mirror ' + m.error); continue; }
    const r = solveEdit(settled, m.sketch); const sk = r.sketch;
    const apex = side.kind === 'line' ? side.b : -1;
    const lines = sk.entities.filter((e) => e.kind === 'line' && e.id !== axis.id && (e.a === apex || e.b === apex)).length;
    for (const [n, f] of [['fillet', fillet], ['chamfer', chamfer]] as const) { const x = f(sk, apex, 3); if ('error' in x || !solveEdit(sk, x.sketch).ok || lines !== 2) out.push(`${auto} ${order} ${apexY} ${n}: ${'error' in x ? x.error : 'unsolved'}`); }
  }
  expect(out).toEqual([]);
});
