import { t } from '../i18n/index';
import { quantity } from '../lib/units.svelte';
import { plural } from './format';
// The machining features as the tool bar shows them: which are on, their
// one-line state, and sensible values when switched on.
import type { Bridges, CommonEdges, Cooling, Features, FlyCut, Joints, Kerf, Leads } from '../api';
import { machined, offEverywhere, onAnywhere } from './selection-machining';
import type { MachiningPart } from '../api';

export interface Tool { id: string; short: string; name: string; desc: string; optional: boolean }

export const TOOLS: Tool[] = [
  { id: 'leads', short: t("Entry lead"), name: t("Entry lead"), desc: t("Keep pierce marks off the finished edge."), optional: true },
  { id: 'joints', short: t("Microjoints"), name: t("Microjoints"), desc: t("Leave small gaps to hold parts in place."), optional: true },
  { id: 'cooling', short: t("Cooling"), name: t("Cooling points"), desc: t("Pause at corners to reduce heat."), optional: true },
  { id: 'kerf', short: t("Kerf"), name: t("Kerf compensation"), desc: t("Offset cuts to allow for the beam width."), optional: true },
  { id: 'bridges', short: t("Bridges"), name: t("Bridges"), desc: t("Join contours with a channel."), optional: true },
  { id: 'common', short: t("Common edges"), name: t("Common edges"), desc: t("Cut selected shared edges once."), optional: true },
  { id: 'fly', short: t("Flying cut"), name: t("Flying cut"), desc: t("Cut straight edges on one line in a single pass, the laser off over the gaps."), optional: true },
  { id: 'order', short: t("Cut order"), name: t("Cutting order"), desc: t("Choose the sequence of cuts."), optional: false },
  { id: 'start', short: t("Start & seam"), name: t("Start point and seam"), desc: t("Set start points, direction, and seam treatment."), optional: false },
];

export const defaultLeads = (): Leads => ({ entry: { shape: 'line', length: 2, radius: 1, angle: 45 }, exit: { shape: 'line', length: 1, radius: 1, angle: 45 }, side: 'auto', closed_only: true, clearance: null, overrides: [] });
export const defaultJoints = (): Joints => ({ placement: { count: 2 }, width: 0.4, minimum_size: 40, outer_only: true, open_start: false, behaviour: 'laser_off', slow_speed: null, repierce: false });
export const defaultCooling = (): Cooling => ({ dwell: 300, placement: { automatic: { at_start: true, corners_below: 60 } } });
export const defaultKerf = (): Kerf => ({ width: 0.2, side: 'auto' });
export const defaultBridges = (): Bridges => ({ width: 2, connections: [] });
export const defaultCommon = (contours: number[]): CommonEdges => ({ contours: [...contours], tolerance: 0.01, allow_overcut: false });
export const defaultFly = (contours: number[]): FlyCut => ({ contours: [...contours], run_up: 3, overcut: 0, tolerance: 0.01, one_way: false, from_far_side: false, directions_reversed: false });

export function isOn(features: Features, id: string): boolean {
  switch (id) {
    case 'leads': return features.leads !== null;
    case 'joints': return features.joints !== null;
    case 'cooling': return features.cooling !== null;
    case 'kerf': return features.kerf !== null;
    case 'bridges': return features.bridges !== null;
    case 'common': return features.common !== null;
    case 'fly': return !!features.fly;
    default: return true;
  }
}

export function toggled(features: Features, id: string, contours: number[] = [], everywhere = false): Features {
  const f = { ...features };
  // For the whole job, off means off on every contour: layers and selections
  // with machining of their own lose it too, and on is the job's again.
  if (everywhere && (id === 'leads' || id === 'joints' || id === 'cooling' || id === 'kerf') && onAnywhere(features, id)) {
    offEverywhere(f, id);
    return f;
  }
  if (id === 'leads') f.leads = f.leads ? null : defaultLeads();
  if (id === 'joints') f.joints = f.joints ? null : defaultJoints();
  if (id === 'cooling') f.cooling = f.cooling ? null : defaultCooling();
  if (id === 'kerf') f.kerf = f.kerf ? null : defaultKerf();
  if (id === 'bridges') f.bridges = f.bridges ? null : defaultBridges();
  if (id === 'common') f.common = f.common ? null : defaultCommon(contours);
  // Undefined, not deleted: callers merge this into the draft's own features.
  if (id === 'fly') f.fly = f.fly ? undefined : defaultFly(contours);
  return f;
}

/** The words the operator reads for a choice the server names by enum. */
export const WORDS: Record<string, string> = {
  keep: 'As drawn', automatic: 'Automatic', manual: 'Manual',
  clockwise: 'Clockwise', counterclockwise: 'Counterclockwise', reverse: 'Reversed',
  as_drawn: 'As drawn', nearest: 'Shortest path', left_to_right: 'Left to right', right_to_left: 'Right to left', bottom_to_top: 'Bottom to top', top_to_bottom: 'Top to bottom',
  line: 'Line', arc: 'Arc', line_arc: 'Line and arc',
};
const title = (s: string) => t(WORDS[s] ?? s.replace(/_/g, ' '));

export function stateOf(features: Features, id: string): string {
  switch (id) {
    case 'leads': { const l = features.leads; if (!l) return t("Off"); const e = l.entry; return e ? `${title(e.shape)} ${quantity(e.length, 'mm')}` : t("Exit only"); }
    case 'joints': {
      const j = features.joints; if (!j) return t("Off");
      const p = j.placement;
      const n = 'count' in p ? t("{0} per contour", p.count) : 'spacing' in p ? t("every {0}", quantity(p.spacing, 'mm')) : 'across_x' in p ? t("{0} across X", p.across_x) : 'across_y' in p ? t("{0} across Y", p.across_y) : t("{0} placed", p.manual.length);
      return `${n} · ${quantity(j.width, 'mm')}`;
    }
    case 'cooling': { const c = features.cooling; if (!c) return t("Off"); return `${(c.dwell / 1000).toFixed(1)} s · ${'manual' in c.placement ? t("{0} placed", c.placement.manual.length) : t("corners")}`; }
    case 'kerf': { const k = features.kerf; return k ? `${quantity(k.width, 'mm')}` : t("Off"); }
    case 'bridges': { const b = features.bridges; return b ? `${b.connections.length} × ${quantity(b.width, 'mm')}` : t("Off"); }
    case 'common': { const c = features.common; return c ? `${plural(c.contours.length, 'contour')} · ${quantity(c.tolerance, 'mm')}` : t("Off"); }
    case 'fly': { const f = features.fly; return f ? t("{0} · run-up {1}", plural(f.contours.length, 'contour'), quantity(f.run_up, 'mm')) : t("Off"); }
    case 'order': { const o = features.order; return typeof o.strategy === 'string' ? (o.inner_first ? t("Inner first") : title(o.strategy)) : t("Manual"); }
    case 'start': { const s = features.start; const at = typeof s.position === 'string' ? title(s.position) : t("Manual"); return `${s.spots.length ? t("{0} chosen", s.spots.length) : at} · ${title(s.direction)}`; }
    default: return '';
  }
}

/** Feedback includes the settings actually used by each layer. */
export function toolbarState(features: Features, id: string, layers: readonly string[]): string {
  const base=stateOf(features,id);
  if(!['leads','joints','cooling','kerf','start'].includes(id))return base;
  const own=(features.layers ?? []).filter(l=>layers.includes(l.name) && l.machining);
  const active=own.flatMap(l=>{
    const m=l.machining!;
    const view={...features,leads:m.leads,joints:m.joints,cooling:m.cooling,kerf:m.kerf,start:m.start,seam:m.seam};
    const state=stateOf(view,id);
    return isOn(view,id) && state!==base ? [l.name+': '+state] : [];
  });
  // Selections with their own setting of this feature, as one line.
  const chosen=(features.contour_machining ?? []).filter(m=>m.own.includes(id as MachiningPart));
  if(chosen.length){
    const count=chosen.reduce((n,m)=>n+m.contours.length,0);
    const states=[...new Set(chosen.map(m=>stateOf(machined(features,m.machining,[id as MachiningPart]),id)))];
    active.push(t('Selection ({0})', count)+': '+states.join(', '));
  }
  return active.length ? active.join(' · ') : base;
}

export function toolbarOn(features: Features,id:string):boolean {
 if(isOn(features,id))return true;
 return (id==='leads'||id==='joints'||id==='cooling'||id==='kerf') && onAnywhere(features,id);
}
