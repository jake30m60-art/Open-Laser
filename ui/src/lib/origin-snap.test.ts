import { describe, expect, it } from 'vitest';
import { originCandidates, snapOrigin } from './origin-snap';
import type { PreviewContour } from '../api';

const contour = (points: [number, number][], closed = true): PreviewContour => ({
  lead_target: null, matching_contour: null, closed, depth: 0, layer: '0', sources: [0],
  paths: [{ kind: 'cut', points }], cooling: [], start: points[0]!, problem: null,
});

describe('origin snapping', () => {
  const square = contour([[0, 0], [10, 0], [10, 10], [0, 10], [0, 0]]);
  const circle = contour(Array.from({ length: 37 }, (_, i) => {
    const a = (i / 36) * 2 * Math.PI;
    return [50 + 5 * Math.cos(a), 20 + 5 * Math.sin(a)] as [number, number];
  }));
  const candidates = originCandidates([square, circle]);

  it('takes a corner near the tap', () => {
    expect(snapOrigin([9.6, 0.3], candidates, 1)).toEqual([10, 0]);
  });
  it('takes a hole centre and not points along its arc', () => {
    const c = snapOrigin([50.4, 19.7], candidates, 1);
    expect(c[0]).toBeCloseTo(50, 6);
    expect(c[1]).toBeCloseTo(20, 6);
    expect(candidates.length).toBe(5);
  });
  it('keeps the tapped point when nothing is near', () => {
    expect(snapOrigin([30, 30], candidates, 1)).toEqual([30, 30]);
  });
});
