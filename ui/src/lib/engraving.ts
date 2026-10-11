// Engraving recipes: library recipes marked for engraving rather than
// cutting, listed apart in the library and offered first for a marked
// layer. Each carries the scan fill it engraves with, which the layer takes
// when the recipe is chosen for it.
import type { Fill } from '../api';
import { defaultFill } from './drawing-layers';

/** The attribute that marks a recipe's process; `engrave` makes it an engraving recipe. */
export const PROCESS = 'OpenLaserProcess';

type Attributes = Readonly<Record<string, string | undefined>>;

/** Whether two recipe names are the same material, whatever the source
 *  appends in brackets: "Stainless Steel (Gweike)" is stainless steel. */
export function sameMaterial(a: string, b: string | undefined): boolean {
  const base = (name: string) => name.replace(/\s*\([^)]*\)\s*$/, '').trim().toLowerCase();
  return b !== undefined && base(a) === base(b);
}

export const isEngraving = (recipe: { attributes: Attributes }): boolean => recipe.attributes[PROCESS] === 'engrave';

/** The fill an engraving recipe brings, over the default fill. */
export function fillOf(attributes: Attributes): Fill {
  const base = defaultFill();
  const number = (key: string, fallback: number): number => {
    const text = attributes[key];
    const value = Number(text);
    return text !== undefined && text.trim() !== '' && Number.isFinite(value) ? value : fallback;
  };
  const flag = (key: string, fallback: boolean): boolean => (attributes[key] === undefined ? fallback : attributes[key] === '1');
  return {
    ...base,
    spacing: Math.min(20, Math.max(0.01, number('OpenLaserFillSpacing', base.spacing))),
    angle: number('OpenLaserFillAngle', base.angle) % 180,
    run_up: Math.min(1000, Math.max(0, number('OpenLaserFillOverscan', base.run_up))),
    bidirectional: flag('OpenLaserFillBidirectional', base.bidirectional),
    outline: flag('OpenLaserFillOutline', base.outline),
    passes: Math.min(20, Math.max(1, Math.round(number('OpenLaserFillPasses', base.passes)))),
    pass_angle: number('OpenLaserFillPassAngle', base.pass_angle) % 360,
    pass_shift: Math.min(99, Math.max(0, number('OpenLaserFillPassShift', base.pass_shift * 100))) / 100,
  };
}

/** What marking a recipe for engraving adds: the default fill, spelled out. */
export function engravingAttributes(): Record<string, string> {
  const f = defaultFill();
  return {
    [PROCESS]: 'engrave',
    OpenLaserFillSpacing: String(f.spacing),
    OpenLaserFillAngle: String(f.angle),
    OpenLaserFillOverscan: String(f.run_up),
    OpenLaserFillBidirectional: f.bidirectional ? '1' : '0',
    OpenLaserFillOutline: f.outline ? '1' : '0',
    OpenLaserFillPasses: String(f.passes),
    OpenLaserFillPassAngle: String(f.pass_angle),
    OpenLaserFillPassShift: String(Math.round(f.pass_shift * 100)),
  };
}

/** The compensation the calibration table gives a fill at `speed` and
 *  `angle`: by speed within each measured angle, then between angles round
 *  half a turn, as the server applies it. None without measurements. */
export function offsetAt(entries: ReadonlyArray<{ angle: number; speed: number; offset: number }>, speed: number, angle: number): number | null {
  if (!entries.length) return null;
  const angles = [...new Set(entries.map((e) => e.angle))].sort((a, b) => a - b);
  const bySpeed = angles.map((a) => {
    const row = entries.filter((e) => e.angle === a).sort((p, q) => p.speed - q.speed);
    const first = row[0]!, last = row.at(-1)!;
    if (speed <= first.speed) return [a, first.offset] as const;
    if (speed >= last.speed) return [a, last.offset] as const;
    const i = row.findIndex((e) => e.speed >= speed);
    const lo = row[i - 1]!, hi = row[i]!;
    return [a, lo.offset + ((hi.offset - lo.offset) * (speed - lo.speed)) / (hi.speed - lo.speed)] as const;
  });
  const at = ((angle % 180) + 180) % 180;
  const ring = [[bySpeed.at(-1)![0] - 180, bySpeed.at(-1)![1]], ...bySpeed, [bySpeed[0]![0] + 180, bySpeed[0]![1]]] as Array<readonly [number, number]>;
  for (let i = 1; i < ring.length; i++) {
    const [a0, o0] = ring[i - 1]!, [a1, o1] = ring[i]!;
    if (at <= a1) return a1 - a0 < 1e-9 ? o1 : o0 + ((o1 - o0) * (at - a0)) / (a1 - a0);
  }
  return bySpeed.at(-1)![1];
}

/** Every how-many-th line of a fill to draw so lines stay `pixels` apart on
 *  screen: 1 draws them all. */
export function hatchStride(spacing: number, mmPerPixel: number, pixels = 4): number {
  if (!(spacing > 0) || !(mmPerPixel > 0)) return 1;
  return Math.max(1, Math.ceil((pixels * mmPerPixel) / spacing));
}

/** The engraved marks of a fill to draw, every `stride`-th line kept: a
 *  line is told by where it lies across its own direction, so passes at
 *  other angles thin alike. */
export function thinned<T extends { points: ReadonlyArray<readonly number[]> }>(marks: readonly T[], spacing: number, stride: number): T[] {
  if (stride <= 1) return [...marks];
  return marks.filter((mark) => {
    const a = mark.points[0], b = mark.points.at(-1);
    if (!a || !b) return false;
    const dx = b[0]! - a[0]!, dy = b[1]! - a[1]!, length = Math.hypot(dx, dy);
    if (!length) return false;
    const across = (a[0]! * -dy + a[1]! * dx) / length;
    return ((Math.round(across / spacing) % stride) + stride) % stride === 0;
  });
}

/** A fill's paths thinned for the screen: every `stride`-th engraved line,
 *  with the run-ups, gaps and turns that follow a kept line. */
export function thinFill<T extends { kind: string; points: ReadonlyArray<readonly number[]> }>(paths: readonly T[], spacing: number, stride: number): T[] {
  if (stride <= 1) return [...paths];
  const kept = new Set(thinned(paths.filter((p) => p.kind === 'cut'), spacing, stride));
  const out: T[] = [];
  let last = true;
  for (const path of paths) {
    if (path.kind === 'cut') { last = kept.has(path); if (last) out.push(path); }
    else if (last) out.push(path);
  }
  return out;
}

/** Display scan lines at readable screen spacing. Collinear pieces separated by
 * a hole stay together; directions from crosshatching retain independent rows. */
export function screenHatch<T extends { points: ReadonlyArray<readonly number[]> }>(paths: readonly T[], mmPerPixel: number): T[] {
  const step = Math.max(0.001, mmPerPixel * 4);
  const rows = new Map<string, number>();
  return paths.filter(path => {
    const a = path.points[0], b = path.points.at(-1);
    if (!a || !b) return false;
    let dx = b[0]! - a[0]!, dy = b[1]! - a[1]!;
    const length = Math.hypot(dx, dy);
    if (length < 1e-9) return false;
    dx /= length; dy /= length;
    if (dx < -1e-9 || (Math.abs(dx) < 1e-9 && dy < 0)) { dx = -dx; dy = -dy; }
    const across = -dy * a[0]! + dx * a[1]!;
    const key = `${Math.round(Math.atan2(dy, dx) * 1e6)}/${Math.floor(across / step)}`;
    const first = rows.get(key);
    if (first === undefined) { rows.set(key, across); return true; }
    return Math.abs(first - across) < 0.001;
  });
}
