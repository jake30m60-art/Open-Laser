import { describe, expect, it } from 'vitest';
import { area, density, estimate, materialPrice, pricePerM2 } from './costing';

const fmt = { sheets: () => '', remnant: () => '', hours: () => '', handling: () => '', setup: () => '', gas: () => '' };
const rect = (w: number, h: number): [number, number][] => [[0, 0], [w, 0], [w, h], [0, h]];
const rates = { machine_per_hour: 60, setup_per_job: 10, minutes_per_sheet: 6, margin_percent: 10 };

describe('job costs', () => {
  it('prices steel by weight from its density', () => {
    expect(density('Carbon Steel (Gweike)', {})).toBe(7.85);
    expect(density('Stainless Steel', {})).toBe(7.93);
    const price = materialPrice('Carbon Steel', { OpenLaserPricePerKg: '1.5' })!;
    expect(pricePerM2(price, 10)).toBeCloseTo(117.75);
    expect(materialPrice('Acrylic', { OpenLaserPricePerM2: '40', OpenLaserPricePerKg: '9' })).toEqual({ perM2: 40 });
    expect(materialPrice('Unobtainium', { OpenLaserPricePerKg: '2' })).toBeNull();
    expect(area(rect(3000, 1500))).toBe(4.5e6);
  });

  it('charges whole sheets, credits the last sheet\'s rest and splits per part', () => {
    const r = estimate({
      rates, price: { perM2: 100 }, thickness: 10, sheet: rect(3000, 1500), cutouts: [],
      sheets: 2, partsPerSheet: [10, 5], active: 1, seconds: 1800, gas: 3,
      used: { minX: 0, minY: 0, maxX: 1000, maxY: 1500 },
    }, fmt);
    const amount = (k: string) => r.lines.find((l) => l.key === k)?.amount;
    expect(amount('material')).toBeCloseTo(900);
    expect(amount('remnant')).toBeCloseTo(-300); // 2000 × 1500 mm back to stock
    expect(amount('machine')).toBeCloseTo(90); // 0.5 h shown + 1 h for twice the parts
    expect(amount('gas')).toBeCloseTo(9);
    expect(amount('handling')).toBeCloseTo(12);
    expect(r.net).toBeCloseTo(900 - 300 + 9 + 90 + 12 + 10);
    expect(r.total).toBeCloseTo(r.net * 1.1);
    expect(r.parts).toBe(15);
    expect(r.perPart).toBeCloseTo(r.total / 15);
    expect(r.scaled).toBe(true);
  });

  it('counts an unpriced cost as zero and says so', () => {
    const r = estimate({
      rates, price: null, thickness: 2, sheet: rect(1000, 500), cutouts: [], sheets: 1,
      partsPerSheet: [4], active: 0, seconds: 0, gas: null, used: { minX: 0, minY: 0, maxX: 950, maxY: 500 },
    }, fmt);
    expect(r.missing).toEqual(['material', 'gas']);
    expect(r.lines.some((l) => l.key === 'remnant')).toBe(false);
  });
});
