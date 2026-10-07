// Engraving recipes: library recipes marked for engraving rather than
// cutting, listed apart in the library and offered first for a marked
// layer. Each carries the scan fill it engraves with, which the layer takes
// when the recipe is chosen for it.
import type { Fill } from '../api';
import { defaultFill } from './drawing-layers';

/** The attribute that marks a recipe's process; `engrave` makes it an engraving recipe. */
export const PROCESS = 'OpenLaserProcess';

type Attributes = Readonly<Record<string, string | undefined>>;

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
