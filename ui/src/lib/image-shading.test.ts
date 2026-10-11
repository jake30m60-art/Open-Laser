import { describe, expect, it } from 'vitest';
import { cellGrid, darkness, shadeCells } from './image-shading';
import type { ImageFill } from '../api';

const base: ImageFill = { mode: 'grayscale', min_power: 10, max_power: 100, invert: false, gamma: 1 };

describe('image shading', () => {
  it('maps black to the most power, white to none and gray between', () => {
    expect([...shadeCells([0, 255, 128], 3, 1, base)]).toEqual([100, 0, 55]);
  });

  it('inverts and bends the midtones as the server does', () => {
    expect(darkness(1, { ...base, invert: true })).toBe(1);
    expect(darkness(0.5, { ...base, gamma: 2 })).toBeCloseTo(0.25);
  });

  it('cuts at middle gray in black and white', () => {
    expect([...shadeCells([100, 160], 2, 1, { ...base, mode: 'threshold', max_power: 80 })]).toEqual([80, 0]);
  });

  it('dithers a mid gray to about half its cells on at full power', () => {
    const gray = new Array(400).fill(128);
    const cells = shadeCells(gray, 20, 20, { ...base, mode: 'dither' });
    const on = cells.filter((c) => c === 100).length;
    expect(cells.every((c) => c === 0 || c === 100)).toBe(true);
    expect(on).toBeGreaterThan(160);
    expect(on).toBeLessThan(240);
  });

  it('sizes the grid by the line spacing within the screen limit', () => {
    expect(cellGrid(30, 20, 0.1)).toEqual({ width: 300, height: 200 });
    expect(cellGrid(300, 150, 0.1, 1200)).toEqual({ width: 1200, height: 600 });
  });
});
