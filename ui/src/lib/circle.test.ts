import { describe, expect, it } from 'vitest';
import { circleThrough } from './circle';

describe('circleThrough', () => {
  it('finds the centre and diameter of three points on a circle', () => {
    const c = circleThrough([[10, 0], [0, 10], [-10, 0]])!;
    expect(c.center[0]).toBeCloseTo(0);
    expect(c.center[1]).toBeCloseTo(0);
    expect(c.diameter).toBeCloseTo(20);
  });
  it('refuses points on a line', () => {
    expect(circleThrough([[0, 0], [1, 1], [2, 2]])).toBeNull();
  });
});
