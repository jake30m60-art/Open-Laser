import { describe, expect, it } from 'vitest';
import { originCandidates, snapOrigin } from './origin-snap';
import type { PreviewContour } from '../api';

type XY = [number, number];
const contour = (points: XY[], closed = true): PreviewContour => ({
  lead_target: null, matching_contour: null, closed, depth: 0, layer: '0', sources: [0],
  paths: [{ kind: 'cut', points }], cooling: [], start: points[0]!, problem: null,
});
const arc = (cx: number, cy: number, r: number, from: number, to: number, steps = 12): XY[] =>
  Array.from({ length: steps + 1 }, (_, i) => {
    const a = (from + (to - from) * i / steps) * Math.PI / 180;
    return [cx + r * Math.cos(a), cy + r * Math.sin(a)];
  });

describe('origin snapping', () => {
  const square = contour([[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]]);
  const circle = contour(arc(50, 20, 5, 0, 360, 36));
  // A slot: two straight sides and two half-round ends, centres at 100 and 120.
  const slot = contour([...arc(120, 0, 3, -90, 90), ...arc(100, 0, 3, 90, 270)]);
  const cross = contour([[200, -10], [200, 10]], false);
  const bar = contour([[190, 0], [210, 0]], false);
  const candidates = originCandidates([square, circle, slot, cross, bar], { min: { x: 0, y: -10 }, max: { x: 210, y: 25 } });
  const near = (at: XY) => snapOrigin(at, candidates, 1);

  it('takes a corner near the tap', () => {
    expect(near([9.6, 0.3])).toEqual({ at: [10, 0], kind: 'corner' });
  });
  it('takes the middle of an edge', () => {
    expect(near([5.2, 0.2])).toEqual({ at: [5, 0], kind: 'middle' });
  });
  it('takes a hole centre', () => {
    const c = near([50.4, 19.7])!;
    expect(c.kind).toBe('centre');
    expect(c.at[0]).toBeCloseTo(50, 6);
    expect(c.at[1]).toBeCloseTo(20, 6);
  });
  it("takes the centres of a slot's round ends and of the slot", () => {
    for (const x of [100, 120]) {
      const c = near([x + 0.3, 0.2])!;
      expect(c.kind).toBe('centre');
      expect(c.at[0]).toBeCloseTo(x, 6);
      expect(c.at[1]).toBeCloseTo(0, 6);
    }
    const middle = near([110.2, 0.1])!;
    expect(middle.kind).toBe('shape');
    expect(middle.at[0]).toBeCloseTo(110, 6);
  });
  it('takes the centre of a square', () => {
    expect(near([5.3, 5.2])).toEqual({ at: [5, 5], kind: 'shape' });
  });
  it('takes where two lines cross, and ends of open lines', () => {
    expect(near([200.3, 0.2])).toEqual({ at: [200, 0], kind: 'crossing' });
    expect(near([190.2, 0.2])).toEqual({ at: [190, 0], kind: 'end' });
  });
  it("takes the part's own corners and middle", () => {
    expect(near([105.2, 7.3])).toEqual({ at: [105, 7.5], kind: 'part' });
  });
  it('gives nothing when no point is near', () => {
    expect(near([30, 30])).toBeNull();
  });
});
