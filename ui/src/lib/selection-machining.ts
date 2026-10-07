// Machining a selection of contours sets for itself, over its layer's and
// the job's: leads inside on one hole, joints on some parts. The server keeps
// these in `contour_machining`; each entry names the parts it sets, and the
// contours follow their layer for the rest. A contour belongs to one entry.
import type { ContourMachining, Features, Machining, MachiningPart } from '../api';
import { scoped } from './drawing-layers';

/** The features that can be a selection's own, as the panels name them. */
export const SELECTION_MACHINING = ['leads', 'joints', 'cooling', 'kerf', 'start'] as const satisfies readonly MachiningPart[];

const BARE: Machining = { leads: null, joints: null, cooling: null, kerf: null, start: { position: 'keep', direction: 'keep', spots: [] }, seam: 'seal' };

/** The entry the contour belongs to, if any. */
export const selectionOf = (features: Features, contour: number): ContourMachining | undefined =>
  features.contour_machining?.find((m) => m.contours.includes(contour));

/** `features` with the `parts` of `own` in their place, keeping the picked
 *  places: lead edits and chosen starts stay the job's. */
export function machined(features: Features, own: Machining, parts: readonly MachiningPart[]): Features {
  const next = { ...features };
  for (const part of parts) {
    if (part === 'leads') next.leads = own.leads ? { ...own.leads, overrides: features.leads?.overrides ?? [] } : null;
    if (part === 'joints') next.joints = own.joints;
    if (part === 'cooling') next.cooling = own.cooling;
    if (part === 'kerf') next.kerf = own.kerf;
    if (part === 'start') { next.start = { ...own.start, spots: features.start.spots }; next.seam = own.seam; }
  }
  return next;
}

/** The features as the contour sees them: its selection's, else its
 *  layer's, else the job's. */
export function selectionView(features: Features, contour: number, layer: string | null): Features {
  const base = scoped(features, layer);
  const own = selectionOf(features, contour);
  return own ? machined(base, own.machining, own.own) : base;
}

/** The part of `view` a selection keeps, without picked places. */
function partOf(machining: Machining, view: Features, part: MachiningPart): Machining {
  const next = structuredClone(machining);
  if (part === 'leads') next.leads = view.leads ? { ...structuredClone(view.leads), overrides: [] } : null;
  if (part === 'joints') next.joints = structuredClone(view.joints);
  if (part === 'cooling') next.cooling = structuredClone(view.cooling);
  if (part === 'kerf') next.kerf = structuredClone(view.kerf);
  if (part === 'start') { next.start = { ...structuredClone(view.start), spots: [] }; next.seam = structuredClone(view.seam); }
  return next;
}

const same = (a: readonly number[], b: readonly number[]) => a.length === b.length && a.every((x) => b.includes(x));

/** Gives `contours` the `part` of `view` as their own. Contours chosen
 *  together share one entry; a contour leaves any entry it was in, taking
 *  along what that entry set, so a second setting adds to the first. */
export function setSelection(features: Features, contours: readonly number[], part: MachiningPart, view: Features): void {
  if (!contours.length) return;
  const list = (features.contour_machining ?? []).map((m) => structuredClone(m));
  const exact = list.find((m) => same(m.contours, contours));
  if (exact) {
    exact.machining = partOf(exact.machining, view, part);
    if (!exact.own.includes(part)) exact.own.push(part);
    features.contour_machining = list;
    return;
  }
  const before = list.find((m) => m.contours.includes(contours[0]!));
  const entry: ContourMachining = {
    contours: [...contours],
    own: before ? [...before.own] : [],
    machining: partOf(before ? before.machining : BARE, view, part),
  };
  if (!entry.own.includes(part)) entry.own.push(part);
  for (const m of list) m.contours = m.contours.filter((c) => !contours.includes(c));
  features.contour_machining = [...list.filter((m) => m.contours.length), entry];
}

/** The contours follow their layer or the job again for `part`. */
export function clearSelection(features: Features, contours: readonly number[], part: MachiningPart): void {
  const list = (features.contour_machining ?? []).map((m) => structuredClone(m));
  const kept: ContourMachining[] = [];
  for (const m of list) {
    const chosen = m.contours.filter((c) => contours.includes(c));
    if (!chosen.length || !m.own.includes(part)) { kept.push(m); continue; }
    const rest = m.contours.filter((c) => !contours.includes(c));
    if (rest.length) kept.push({ ...m, contours: rest });
    const own = m.own.filter((p) => p !== part);
    if (own.length) kept.push({ ...m, contours: chosen, own });
  }
  features.contour_machining = kept;
}

/** Whether any of the contours sets `part` itself. */
export const selectionHasOwn = (features: Features, contours: readonly number[], part: MachiningPart): boolean =>
  !!features.contour_machining?.some((m) => m.own.includes(part) && m.contours.some((c) => contours.includes(c)));

/** Switches `part` off everywhere: the job's, every layer's own and every
 *  selection's, so off means no such machining on any contour. */
export function offEverywhere(features: Features, part: MachiningPart): void {
  const clear = (m: Machining) => {
    if (part === 'leads') m.leads = null;
    if (part === 'joints') m.joints = null;
    if (part === 'cooling') m.cooling = null;
    if (part === 'kerf') m.kerf = null;
  };
  if (part === 'leads') features.leads = null;
  if (part === 'joints') features.joints = null;
  if (part === 'cooling') features.cooling = null;
  if (part === 'kerf') features.kerf = null;
  features.layers = features.layers?.map((l) => {
    if (!l.machining) return l;
    const machining = structuredClone(l.machining);
    clear(machining);
    return { ...l, machining };
  });
  features.contour_machining = features.contour_machining
    ?.map((m) => ({ ...m, own: m.own.filter((p) => p !== part) }))
    .filter((m) => m.own.length);
}

/** Whether `part` is on anywhere: for the job, a layer of its own or a selection. */
export function onAnywhere(features: Features, part: MachiningPart): boolean {
  const of = (m: Machining) => (part === 'leads' ? m.leads : part === 'joints' ? m.joints : part === 'cooling' ? m.cooling : part === 'kerf' ? m.kerf : null) !== null;
  return of(features as unknown as Machining)
    || !!features.layers?.some((l) => l.machining && of(l.machining))
    || !!features.contour_machining?.some((m) => m.own.includes(part) && of(m.machining));
}
