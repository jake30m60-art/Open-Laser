import { sketchStore } from '../sketch/store.svelte';
import { emptySketch } from '../sketch/model';
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
    const {name,drawing} = await api.partDrawing(id);
    sketchStore.begin(fromDrawing(drawing),name+' – '+t('Edited'),'new');
    if(contour !== undefined && drawing.contours[contour]) {
      const first=drawing.contours.slice(0,contour).reduce((n,c)=>n+c.curves.length,0);
      sketchStore.select(sketchStore.sketch.entities.slice(first,first+drawing.contours[contour]!.curves.length).map(e=>e.id));
    }
    ui.setupPanel = null; ui.tab = 'sketch';
    ui.say(t('Editing source geometry. Save creates a new part and opens a new project; the original remains available.'));
  } catch (error) { ui.say(explain(error),true); }
}
