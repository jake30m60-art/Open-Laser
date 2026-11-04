import { describe, expect, it } from 'vitest';
import { circleThrough } from './circle';

describe('circleThrough', () => {
  it('finds the centre and diameter of three points on a circle', () => {
    const c = circleThrough([[10, 0], [0, 10], [-10, 0]])!;
    expect(c.center[0]).toBeCloseTo(0);
    expect(c.center[1]).toBeCloseTo(0);
    expect(c.diameter).toBeCloseTo(20);
  });
  it('fits four points and tells how far one lies off', () => {
    const c = circleThrough([[470, 100], [530, 100], [500, 70], [500, 130]])!;
    expect(c.center[0]).toBeCloseTo(500);
    expect(c.center[1]).toBeCloseTo(100);
    expect(c.diameter).toBeCloseTo(60);
    expect(c.off).toBeCloseTo(0);
    expect(circleThrough([[470, 100], [530, 100], [500, 70], [500, 136]])!.off).toBeGreaterThan(1);
  });
  it('refuses points on a line', () => {
    expect(circleThrough([[0, 0], [1, 1], [2, 2]])).toBeNull();
  });
});
