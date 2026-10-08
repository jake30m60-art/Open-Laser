import { beforeAll, describe, expect, it } from 'vitest';
import { addConstraint, addEntity, addPoint, clone, emptySketch, point, type Sketch } from './model';
import { initSolver, solveEdit } from './solver';

beforeAll(async () => { await initSolver(); });

/** Signed area of the polygon through these points: its turning sense. */
const area = (s: Sketch, ids: number[]) => ids.reduce((sum, id, i) => {
  const a = point(s, id), b = point(s, ids[(i + 1) % ids.length]!);
  return sum + a.x * b.y - b.x * a.y;
}, 0) / 2;

describe('large dimension changes keep the shape on its side', () => {
  it('opens an angle from 30 to 150 and 175 degrees without mirroring the rotated line', () => {
    for (const target of [90, 150, 175]) {
      const s = emptySketch();
      const o = addPoint(s, 0, 0), x = addPoint(s, 50, 0), y = addPoint(s, 50 * Math.cos(Math.PI / 6), 50 * Math.sin(Math.PI / 6));
      const base = addEntity(s, { kind: 'line', a: o, b: x });
      const arm = addEntity(s, { kind: 'line', a: o, b: y });
      addConstraint(s, { kind: 'horizontal', line: base });
      addConstraint(s, { kind: 'distance', a: o, b: y, axis: 'aligned', value: 50, at: [0, 0] });
      addConstraint(s, { kind: 'angle', a: base, b: arm, value: 30, at: [0, 0] });
      const start = solveEdit(s, s);
      expect(start.ok).toBe(true);
      const next = clone(start.sketch);
      const angle = next.constraints.find((c) => c.kind === 'angle')!;
      if (angle.kind !== 'angle') throw Error('angle');
      angle.value = target;
      const out = solveEdit(start.sketch, next);
      expect(out.ok, `to ${target}`).toBe(true);
      const tip = point(out.sketch, y), at = point(out.sketch, o);
      expect(tip.y - at.y, `arm stays above the base at ${target}`).toBeGreaterThan(0);
      expect(Math.atan2(tip.y - at.y, tip.x - at.x) * 180 / Math.PI).toBeCloseTo(target, 3);
    }
  });

  it('stretches a triangle side a lot without turning the triangle over', () => {
    for (const target of [20, 70, 95]) {
      const s = emptySketch();
      const a = addPoint(s, 0, 0), b = addPoint(s, 40, 0), c = addPoint(s, 20, 30);
      const ab = addEntity(s, { kind: 'line', a, b }), bc = addEntity(s, { kind: 'line', a: b, b: c }), ca = addEntity(s, { kind: 'line', a: c, b: a });
      addConstraint(s, { kind: 'horizontal', line: ab });
      addConstraint(s, { kind: 'fix', point: a });
      addConstraint(s, { kind: 'distance', a, b, axis: 'aligned', value: 40, at: [0, 0] });
      addConstraint(s, { kind: 'distance', a: b, b: c, axis: 'aligned', value: Math.hypot(20, 30), at: [0, 0] });
      addConstraint(s, { kind: 'distance', a: c, b: a, axis: 'aligned', value: Math.hypot(20, 30), at: [0, 0] });
      void bc; void ca;
      const start = solveEdit(s, s);
      expect(start.ok).toBe(true);
      const before = Math.sign(area(start.sketch, [a, b, c]));
      const next = clone(start.sketch);
      const side = next.constraints.find((k) => k.kind === 'distance' && k.a === a && k.b === b)!;
      if (side.kind !== 'distance') throw Error('distance');
      side.value = target;
      const out = solveEdit(start.sketch, next);
      expect(out.ok, `to ${target}`).toBe(true);
      expect(Math.sign(area(out.sketch, [a, b, c])), `turning sense at ${target}`).toBe(before);
    }
  });
});
