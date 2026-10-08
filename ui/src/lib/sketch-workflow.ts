import { sketchStore } from '../sketch/store.svelte';
import { server } from '../stores/server.svelte';
import { emptySketch, isSketch } from '../sketch/model';
import { fromDrawing } from '../sketch/from-drawing';
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
    sketchStore.begin(bridged ? fromDrawing(bridged) : isSketch(sketch)?sketch:fromDrawing(drawing),name,replacement?'replace':'new',id,replacement?current?.key:undefined);
    if(!bridged && !sketch && contour !== undefined && drawing.contours[contour]) {
      const first=drawing.contours.slice(0,contour).reduce((n,c)=>n+c.curves.length,0);
      sketchStore.select(sketchStore.sketch.entities.slice(first,first+drawing.contours[contour]!.curves.length).map(e=>e.id));
    }
    ui.setupPanel = null; ui.tab = 'sketch';
    ui.say(bridged ? t('The job’s bridges are drawn into the sketch; saving replaces the part with them in it.')
      : t(replacement?'Saving replaces the part in this job. Undo restores the original; changed contour features need review.':'Editing source geometry as a new library part.'));
  } catch (error) { ui.say(explain(error),true); }
}
