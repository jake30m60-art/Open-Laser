import { describe, expect, it } from 'vitest';
import { distanceAtTime, segmentSeconds, timeAtDistance } from './cut-order';

const seg = (length: number, contour: number) => ({ a: [0, 0] as [number, number], b: [length, 0] as [number, number], contour, length, kind: 'cut' as const });

describe('simulation in machine time', () => {
  // 100 mm at 10 mm/s (10 s), then 100 mm at 100 mm/s (1 s).
  const segments = [seg(100, 0), seg(100, 1)];
  const seconds = segmentSeconds(segments, (s) => (s.contour === 0 ? 10 : 100));
  it('takes each stretch at its own speed', () => {
    expect(seconds).toEqual([10, 1]);
  });
  it('maps time to distance and back', () => {
    expect(distanceAtTime(segments, seconds, 10 / 11)).toBeCloseTo(0.5);
    expect(timeAtDistance(segments, seconds, 0.75)).toBeCloseTo(10.5 / 11);
  });
});
