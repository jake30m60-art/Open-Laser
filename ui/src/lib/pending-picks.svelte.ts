// Places picked on the drawing are kept as soon as the picking session
// ends some other way than Done: switching tools or tabs must not drop a
// chosen start point, joint or cooling stop.
import { api } from '../api/client';
import { ui } from '../stores/ui.svelte';

/** Saves the open picking session's places, if any, and ends it. */
export function keepPendingPicks(): void {
  const picking = ui.picking;
  ui.picking = null;
  if (!picking || !picking.marks.length || picking.previewPending) return;
  if (!['start', 'joints', 'cooling'].includes(picking.feature)) return;
  api.setFeatures($state.snapshot(picking.features), picking.revision).catch(() => {
    ui.say('The picked places were not kept; the drawing changed. Pick them again.', true);
  });
}
