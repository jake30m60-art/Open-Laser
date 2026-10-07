import { describe, expect, it } from 'vitest';
import { arrowAlong } from './direction';

describe('arrowAlong', () => {
  it('points the way the path runs, in the middle of its longest stretch', () => {
    const d = arrowAlong([[0, 0], [10, 0]], 1)!;
    expect(d.startsWith('M5.5 0L4.5 ')).toBe(true);
    const back = arrowAlong([[10, 0], [0, 0]], 1)!;
    expect(back.startsWith('M4.5 0L5.5 ')).toBe(true);
  });

  it('keeps off the corners', () => {
    const d = arrowAlong([[0, 0], [10, 0], [10, 2]], 1)!;
    expect(d.startsWith('M5.5 0L')).toBe(true);
  });

  it('leaves out paths too short for one', () => {
    expect(arrowAlong([[0, 0], [2, 0]], 1)).toBeNull();
    expect(arrowAlong([[0, 0]], 1)).toBeNull();
  });
});
