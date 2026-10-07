import { describe, expect, it } from 'vitest';
import { envelopeFits, foldedScanAngle, scanEnvelope, scanPadding, scanRunUp, scanTestLayout } from './scan-calibration';
describe('scan coupon travel envelope', () => {
  const bed = { min: {x:0,y:0}, max:{x:1000,y:1000} };
  it('rejects an engraving that fits but whose laser-off run-up leaves the bed', () => {
    expect(envelopeFits(scanEnvelope([10,20],[40,4],0,400),bed)).toBe(false);
    expect(envelopeFits(scanEnvelope([100,100],[40,4],0,400),bed)).toBe(true);
  });
  it('reserves space on both axes for angled tests', () => {
    const [x,y]=scanPadding(45,400);
    expect(x).toBeCloseTo(80/Math.sqrt(2));expect(y).toBeCloseTo(x);
    expect(envelopeFits(scanEnvelope([20,10],[40,4],45,400),bed)).toBe(false);
    expect(envelopeFits(scanEnvelope([100,100],[40,4],45,400),bed)).toBe(true);
    expect(scanPadding(90,400)[1]).toBeCloseTo(80);
  });
  it('uses the same angle seam as the saved tables', () => {
    expect(foldedScanAngle(179.99)).toBe(0);expect(foldedScanAngle(-0.01)).toBe(0);
    expect(foldedScanAngle(270)).toBe(90);
  });
});

it('groups twelve lines per speed in 0/45/90 order with overlapping laser-off travel', () => {
  const layout=scanTestLayout([100,100],[200,250],[0,45,90],10,1,6000,0.25);
  expect(layout.tests.map(t=>[t.speed,t.angle])).toEqual([[200,0],[200,45],[200,90],[250,0],[250,45],[250,90]]);
  expect(layout.tests[0]!.corner[1]).toBe(layout.tests[2]!.corner[1]);
  expect(layout.tests[3]!.corner[1]).toBeGreaterThan(layout.tests[0]!.corner[1]);
  const envelopes=layout.tests.map(t=>scanEnvelope(t.corner,t.size,t.angle,t.speed));
  expect(layout.tests[0]!.corner[0] + layout.tests[0]!.size[0]).toBeLessThan(layout.tests[1]!.corner[0]);
  expect(envelopes[0]!.max.x).toBeGreaterThan(envelopes[1]!.min.x);
  expect(scanRunUp(200,6000,0.25)).toBeCloseTo(42.5);
  expect(scanRunUp(200,1000,0.25)).toBeGreaterThan(42.5);
});

 it('packs the default four-speed marks into 44 by 70 mm without shortening ramps', () => {
  const layout = scanTestLayout([0,0],[200,250,300,350],[0,45,90],10,1);
  const size = [0,1].map(a => Math.max(...layout.tests.map(t => t.corner[a]! + t.size[a]!)));
  expect(size[0]).toBeCloseTo(44); expect(size[1]).toBeCloseTo(70);
  const bed = {min:{x:0,y:0},max:{x:200,y:200}};
  const origin = layout.before.map(v => v + 10) as [number,number];
  const placed = scanTestLayout(origin,[200,250,300,350],[0,45,90],10,1);
  expect(placed.tests.every(t => envelopeFits(scanEnvelope(t.corner,t.size,t.angle,t.speed),bed))).toBe(true);
  expect(scanRunUp(350)).toBeCloseTo(70.625);
  expect(layout.before[1]).toBeLessThan(layout.padding[1]);
  const high = scanTestLayout(origin,[2000],[0,45,90],10,1);
  expect(high.tests.every(t => envelopeFits(scanEnvelope(t.corner,t.size,t.angle,t.speed),bed))).toBe(false);
 });
