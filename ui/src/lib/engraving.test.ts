import { describe, expect, it } from 'vitest';
import { hatchStride, offsetAt, thinFill, thinned } from './engraving';

describe('engraving display and compensation', () => {
  it('reads the calibration by speed, then between angles', () => {
    const table = [
      { angle: 0, speed: 100, offset: 0.4 }, { angle: 0, speed: 300, offset: 1.0 },
      { angle: 90, speed: 100, offset: 0.2 }, { angle: 90, speed: 300, offset: 0.6 },
    ];
    expect(offsetAt(table, 200, 0)).toBeCloseTo(0.7);
    expect(offsetAt(table, 200, 90)).toBeCloseTo(0.4);
    expect(offsetAt(table, 200, 45)).toBeCloseTo(0.55);
    expect(offsetAt(table, 50, 0)).toBeCloseTo(0.4);
    expect(offsetAt([], 200, 0)).toBeNull();
    // One angle counts for every direction.
    expect(offsetAt(table.slice(0, 2), 300, 120)).toBeCloseTo(1.0);
  });

  it('thins a dense fill on screen and keeps a sparse one', () => {
    expect(hatchStride(0.1, 0.2)).toBe(8);
    expect(hatchStride(2, 0.2)).toBe(1);
    const marks = Array.from({ length: 20 }, (_, k) => ({ points: [[0, k * 0.1], [10, k * 0.1]] }));
    expect(thinned(marks, 0.1, 5)).toHaveLength(4);
    expect(thinned(marks, 0.1, 1)).toHaveLength(20);
  });

  it('keeps the run-ups and turns of the lines it keeps', () => {
    const paths = Array.from({ length: 10 }, (_, k) => [
      { kind: 'travel', points: [[-5, k * 0.1], [0, k * 0.1]] },
      { kind: 'cut', points: [[0, k * 0.1], [10, k * 0.1]] },
    ]).flat();
    const shown = thinFill(paths, 0.1, 5);
    expect(shown.filter((p) => p.kind === 'cut')).toHaveLength(2);
    expect(shown.filter((p) => p.kind === 'travel').length).toBeGreaterThanOrEqual(2);
    expect(thinFill(paths, 0.1, 1)).toHaveLength(20);
  });
});
