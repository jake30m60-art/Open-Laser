import { sketchStore } from '../sketch/store.svelte';
import { server } from '../stores/server.svelte';
import { emptySketch, isSketch } from '../sketch/model';
import { fromDrawing } from '../sketch/from-drawing';
import { fromSource } from '../sketch/from-source';
import { api } from '../api/client';
import { ui } from '../stores/ui.svelte';
import { explain } from './format';
import { t } from '../i18n/index';
export function beginProject(target: 'new' | 'add' = 'new'): void {
  sketchStore.begin(emptySketch(),t(target === 'new' ? 'New project' : 'New sketch'),target);
  ui.setupPanel = null; ui.tab = 'sketch';
}
export async function editPart(id: string, contour?: number): Promise<void> {
  try {
    const {name,drawing,sketch} = await api.partDrawing(id);
    const current=server.doc?.draft;
    const replacement=!!current?.parts.some(p=>p.id===id);
    // Bridges set in the job become part of the drawing, so they stay when
    // the edited part replaces the old one.
    const bridged=replacement && current?.features.bridges?.connections.length ? (await api.bridgedPart(id)).drawing : null;
    // A DXF part as imported opens with its ellipses and splines as drawn.
    const exact=!bridged && !isSketch(sketch) && contour === undefined ? (await api.partSketchSource(id).catch(() => null))?.curves ?? null : null;
    sketchStore.begin(bridged ? fromDrawing(bridged) : isSketch(sketch)?sketch:exact?fromSource(exact):fromDrawing(drawing),name,replacement?'replace':'new',id,replacement?current?.key:undefined);
    if(!bridged && !sketch && contour !== undefined && drawing.contours[contour]) {
      const first=drawing.contours.slice(0,contour).reduce((n,c)=>n+c.curves.length,0);
      sketchStore.select(sketchStore.sketch.entities.slice(first,first+drawing.contours[contour]!.curves.length).map(e=>e.id));
    }
    ui.setupPanel = null; ui.tab = 'sketch';
    ui.say(bridged ? t('The job’s bridges are drawn into the sketch; saving replaces the part with them in it.')
      : t(replacement?'Saving replaces the part in this job. Undo restores the original; changed contour features need review.':'Editing source geometry as a new library part.'));
    // A drawing of many short pieces (splines, scans) is heavy to edit; say what simplifying would leave.
    const curves=drawing.contours.reduce((n,c)=>n+c.curves.length,0);
    if(!sketch && !bridged && !exact && curves>MANY_CURVES) void simplifyHint(id,curves);
  } catch (error) { ui.say(explain(error),true); }
}

/** How many curves of a drawing make editing it as a sketch slow. */
const MANY_CURVES = 150;
async function simplifyHint(id: string, curves: number): Promise<void> {
  try {
    const view = await api.simplifyPart(id, 0.05, false);
    if (view.curves_after < curves * 0.6) ui.say(t('This drawing has {0} lines and arcs. Parts › Simplify drawing (0.05 mm) would leave {1}; simplify first for a lighter sketch.', curves, view.curves_after), false, true);
  } catch { /* only a hint */ }
}
