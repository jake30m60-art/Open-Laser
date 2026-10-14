import { expect, it } from 'vitest';
import { fromSource } from './from-source';
import { arcParams } from './curves';
import { endsOf, point } from './model';

it('builds a slot of elliptical ends and lines whose ends meet, and a rational spline', () => {
  const s = fromSource([
    { layer: 'cut', kind: 'line', start: [0, -5], end: [20, -5] },
    { layer: 'cut', kind: 'ellipse', center: [20, 0], major: [0, 5], minor_radius: 3, start: -Math.PI, end: 0 },
    { layer: 'cut', kind: 'line', start: [20, 5], end: [0, 5] },
    { layer: 'cut', kind: 'ellipse', center: [50, 50], major: [10, 0], minor_radius: 4, start: 0, end: 2 * Math.PI },
    { layer: 'cut', kind: 'spline', degree: 2, knots: [0, 0, 0, 1, 1, 1], poles: [[20, 5], [30, 15], [40, 5]], weights: [1, 0.7, 1] },
  ]);
  const [low, half, high, whole, spline] = s.entities;
  expect(half!.kind).toBe('ellipse-arc');
  expect(whole!.kind).toBe('ellipse');
  // The half ellipse joins both lines.
  const ends = endsOf(half!)!;
  expect(ends).toContain(endsOf(low!)![1]);
  expect(ends).toContain(endsOf(high!)![0]);
  if (!half || half.kind !== 'ellipse-arc') throw new Error();
  const [from, to] = arcParams(s, half);
  expect(to - from).toBeCloseTo(Math.PI);
  expect(point(s, half.m)).toEqual({ id: half.m, x: 20, y: 5 });
  // The spline starts on the upper line's start and keeps its weights.
  if (!spline || spline.kind !== 'spline') throw new Error();
  expect(spline.poles[0]).toBe(endsOf(high!)![0]);
  expect(spline.weights).toEqual([1, 0.7, 1]);
});
