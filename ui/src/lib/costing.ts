// The job cost estimate: material by whole sheets (the rest of the last
// sheet credited back as a remnant), gas from the server's estimate,
// machine time and handling at the shop's hourly rate, a fixed amount per
// job and a markup. Prices come from the recipe (raw material) and
// Settings → Costs (shop rates); everything is in the shop's one currency.
import type { ShopRates } from '../api';

/** Recipe attributes that carry the raw material price. */
export const PRICE_PER_KG = 'OpenLaserPricePerKg';
export const PRICE_PER_M2 = 'OpenLaserPricePerM2';
export const DENSITY = 'OpenLaserDensity';

/** Typical densities in g/cm³ (= kg per m² and mm), by material name. */
const DENSITIES: [RegExp, number][] = [
  [/stainless|edelstahl|inox|v2a|v4a|1\.4301|1\.4404/i, 7.93],
  [/alu/i, 2.7],
  [/brass|messing/i, 8.5],
  [/copper|kupfer/i, 8.96],
  [/galvan|verzinkt|zinc/i, 7.85],
  [/titan/i, 4.5],
  [/carbon|steel|stahl|mild|s235|s355|dc01/i, 7.85],
  [/acryl|pmma|plexi/i, 1.19],
  [/plywood|sperrholz/i, 0.6],
  [/mdf|density board/i, 0.75],
  [/wood|holz|basswood|linde/i, 0.5],
];

/** The density the recipe names, else the usual one for its material. */
export function density(name: string, attributes: Record<string, string>): number | null {
  const own = Number(attributes[DENSITY]);
  if (own > 0) return own;
  return DENSITIES.find(([pattern]) => pattern.test(name))?.[1] ?? null;
}

/** How the recipe prices its raw material, or null without a price. */
export type MaterialPrice = { perKg: number; density: number } | { perM2: number };
export function materialPrice(name: string, attributes: Record<string, string>): MaterialPrice | null {
  const perM2 = Number(attributes[PRICE_PER_M2]);
  if (perM2 > 0) return { perM2 };
  const perKg = Number(attributes[PRICE_PER_KG]);
  const rho = density(name, attributes);
  return perKg > 0 && rho ? { perKg, density: rho } : null;
}

/** The price of one square metre at `thickness` mm. */
export function pricePerM2(price: MaterialPrice, thickness: number): number {
  return 'perM2' in price ? price.perM2 : price.perKg * price.density * Math.max(0, thickness);
}

/** Shoelace area of a closed polyline, mm². */
export function area(points: [number, number][]): number {
  let sum = 0;
  for (let i = 0; i < points.length; i++) {
    const [x1, y1] = points[i]!, [x2, y2] = points[(i + 1) % points.length]!;
    sum += x1 * y2 - x2 * y1;
  }
  return Math.abs(sum) / 2;
}

/** A remnant strip narrower than this is scrap, not credited. */
export const MIN_REMNANT_MM = 100;

export type CostInput = {
  rates: ShopRates;
  /** Raw material price, or null when the recipe has none. */
  price: MaterialPrice | null;
  thickness: number;
  /** The sheet's outline (a remnant's real shape) and what was cut from it before. */
  sheet: [number, number][];
  cutouts: [number, number][][];
  /** How many sheets the job set has, and which one is shown. */
  sheets: number;
  /** Parts on each sheet, when known; the shown one's always is. */
  partsPerSheet: (number | null)[];
  active: number;
  /** The shown sheet's run time, gas cost (null when unpriced) and the
   *  extent of what it cuts, in drawing coordinates. */
  seconds: number;
  gas: number | null;
  used: { minX: number; maxX: number; minY: number; maxY: number } | null;
};

export type CostLine = { key: 'material' | 'remnant' | 'gas' | 'machine' | 'handling' | 'setup'; amount: number | null; detail: string };
export type CostResult = {
  lines: CostLine[];
  /** Costs without a price yet count as zero and are listed here. */
  missing: CostLine['key'][];
  net: number;
  margin: number;
  total: number;
  parts: number;
  perPart: number | null;
  /** True when other sheets' times are scaled from the shown one's. */
  scaled: boolean;
  sheetArea: number;
  remnantArea: number;
};


/** The estimate for the whole job set; `fmt` writes a detail line's figures. */
export function estimate(input: CostInput, fmt: {
  sheets: (count: number, m2: number) => string;
  remnant: (m2: number) => string;
  hours: (seconds: number, rate: number) => string;
  handling: (sheets: number, minutes: number) => string;
  setup: () => string;
  gas: () => string;
}): CostResult {
  const { rates } = input;
  const sheetArea = Math.max(0, (area(input.sheet) - input.cutouts.reduce((s, c) => s + area(c), 0)) / 1e6);
  const count = Math.max(1, input.sheets);
  const m2Price = input.price ? pricePerM2(input.price, input.thickness) : null;

  // The rest of the last sheet beyond the parts, along the sheet's length,
  // goes back to stock when it is wide enough to use.
  let remnantArea = 0;
  if (input.used && input.active === count - 1 && input.sheet.length >= 3) {
    const xs = input.sheet.map((p) => p[0]), ys = input.sheet.map((p) => p[1]);
    const minX = Math.min(...xs), maxX = Math.max(...xs), minY = Math.min(...ys), maxY = Math.max(...ys);
    const alongX = maxX - minX >= maxY - minY;
    const rest = alongX ? maxX - input.used.maxX : maxY - input.used.maxY;
    if (rest >= MIN_REMNANT_MM) remnantArea = Math.min(sheetArea, (rest * (alongX ? maxY - minY : maxX - minX)) / 1e6);
  }

  // Other sheets run about as long as the shown one, per part on them.
  const shownParts = input.partsPerSheet[input.active] ?? 0;
  let scaled = false;
  const factors = Array.from({ length: count }, (_, i) => {
    if (i === input.active) return 1;
    scaled = true;
    const parts = input.partsPerSheet[i];
    return parts !== null && parts !== undefined && shownParts > 0 ? parts / shownParts : 1;
  });
  const runFactor = factors.reduce((a, b) => a + b, 0);
  const seconds = input.seconds * runFactor;
  const parts = input.partsPerSheet.reduce<number>((s, p, i) => s + (p ?? (i === input.active ? 0 : shownParts)), 0);

  const material = m2Price === null ? null : m2Price * sheetArea * count;
  const credit = m2Price === null || remnantArea <= 0 ? 0 : -m2Price * remnantArea;
  const gas = input.gas === null ? null : input.gas * runFactor;
  const machine = (seconds / 3600) * rates.machine_per_hour;
  const handling = ((count * rates.minutes_per_sheet) / 60) * rates.machine_per_hour;
  const lines: CostLine[] = [
    { key: 'material', amount: material, detail: fmt.sheets(count, sheetArea) },
    ...(credit ? [{ key: 'remnant' as const, amount: credit, detail: fmt.remnant(remnantArea) }] : []),
    { key: 'gas', amount: gas, detail: fmt.gas() },
    { key: 'machine', amount: machine, detail: fmt.hours(seconds, rates.machine_per_hour) },
    ...(rates.minutes_per_sheet > 0 ? [{ key: 'handling' as const, amount: handling, detail: fmt.handling(count, rates.minutes_per_sheet) }] : []),
    ...(rates.setup_per_job > 0 ? [{ key: 'setup' as const, amount: rates.setup_per_job, detail: fmt.setup() }] : []),
  ];
  const net = lines.reduce((s, l) => s + (l.amount ?? 0), 0);
  const margin = (net * rates.margin_percent) / 100;
  const total = net + margin;
  const missing = lines.filter((l) => l.amount === null).map((l) => l.key);
  return { lines, missing, net, margin, total, parts, perPart: parts > 0 ? total / parts : null, scaled, sheetArea, remnantArea };
}
