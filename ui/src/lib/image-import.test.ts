import { describe, expect, it } from 'vitest';
import { grayPixels, heightFor, MAX_SIDE, targetSize } from './image-import';

describe('image import', () => {
  it('weighs colours by brightness and shows transparency as white', () => {
    const gray = grayPixels([0, 0, 0, 255, 255, 255, 255, 255, 255, 0, 0, 255, 0, 0, 0, 0]);
    expect([...gray]).toEqual([0, 255, 76, 255]);
  });

  it('keeps no more pixels than the laser resolves or the server takes', () => {
    // 100 mm resolves 2000 pixels at 0.05 mm each.
    expect(targetSize(6000, 3000, 100)).toEqual({ width: 2000, height: 1000 });
    // A small file stays as it is.
    expect(targetSize(300, 200, 100)).toEqual({ width: 300, height: 200 });
    // A huge print is capped on its long side.
    expect(Math.max(...Object.values(targetSize(20000, 10000, 2000)))).toBe(MAX_SIDE);
  });

  it('keeps the proportions', () => {
    expect(heightFor(400, 300, 80)).toBe(60);
  });
});
