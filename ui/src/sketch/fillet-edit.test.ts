import { beforeAll, expect, it } from 'vitest';
import { emptySketch, point } from './model';
import { createTool } from './tools';
import { fillet } from './modify';
import { initSolver, looseAfter, solve } from './solver';
beforeAll(async () => { await initSolver(); });
/** Changing a fillet's radius moves the fillet, not the rest of the outline. */
it('a new fillet radius leaves the rest of the outline where it was', () => {
  const s = emptySketch(); const t = createTool('rect2');
  t.click(s, { x: 0, y: 0, kind: 'free' }, {}); t.click(s, { x: 100, y: 60, kind: 'free' }, {});
  const corner = s.points.find((p) => p.x === 100 && p.y === 60)!;
  const f = fillet(s, corner.id, 10); if ('error' in f) throw new Error(f.error);
  for (const r of [5, 20, 30, 40]) {
    const sk = JSON.parse(JSON.stringify(f.sketch));
    const dim = sk.constraints.find((c: { kind: string }) => c.kind === 'radius'); dim.value = r;
    const out = solve(sk, [], new Map(), looseAfter(f.sketch, sk));
    const lines: string[] = out.sketch.entities.filter((e) => e.kind === 'line').map((l) => l.kind === 'line' ? [point(out.sketch, l.a), point(out.sketch, l.b)].map((p) => `${(p.x + 0).toFixed(1).replace('-0.0', '0.0')},${(p.y + 0).toFixed(1).replace('-0.0', '0.0')}`).join('>') : '');
    expect(out.ok).toBe(true);
    expect(lines[0]).toBe('0.0,0.0>100.0,0.0');
    expect(lines[1]).toBe(`100.0,0.0>100.0,${(60 - r).toFixed(1)}`);
    expect(lines[2]).toBe(`${(100 - r).toFixed(1)},60.0>0.0,60.0`);
  }
});
