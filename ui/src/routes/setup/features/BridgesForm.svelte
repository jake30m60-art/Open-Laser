<script lang="ts">
  import { ui } from '../../../stores/ui.svelte';
  import { server } from '../../../stores/server.svelte';
  import { t } from '../../../i18n/index';
  import { api } from '../../../api/client';
  import { explain } from '../../../lib/format';

  // Bridges: channels that join two contours into one cut, or split one.
  import Placed from '../../../components/params/Placed.svelte';
  import Stepper from '../../../components/params/Stepper.svelte';
  import { defaultBridges } from '../../../lib/features';
  import type { Features } from '../../../api';
  import type { PickFeature, SetFeatures } from './form';

  let { features, on, set, pick }: {
    features: Features;
    on: boolean;
    set: SetFeatures;
    pick: (feature: PickFeature) => void;
  } = $props();

  const bridges = $derived(features.bridges ?? defaultBridges());
  const error = $derived(server.doc?.draft?.error);
  const bad = $derived(Number(error?.match(/bridge (\d+):/)?.[1] ?? 0));
  let removing = $state(false);
  /** Removes one bridge; the server keeps the others where they are. */
  async function remove(index: number): Promise<void> {
    if (removing) return;
    removing = true;
    try {
      const { dropped } = await api.removeBridge(index);
      ui.say(dropped ? t('Bridge {0} removed; {1} drawn on its channel went with it.', index + 1, dropped) : t('Bridge {0} removed.', index + 1));
    } catch (error) {
      ui.say(explain(error), true);
    } finally {
      removing = false;
    }
  }
  const setWidth = (v: number) => set((f) => { f.bridges!.width = v; });
  const clear = () => set((f) => { f.bridges!.connections = []; });
  const undoLast = () => set((f) => { f.bridges!.connections.pop(); });
</script>

<p class="muted">{t("Click the first edge to open its preview. Snap to the second edge and click to save. Continue with the next bridge; Undo restores the previous cut.")}</p>
<p class="muted">{t("Stencil bridge: connect the outer boundary to the inner hole. Keep separate bridges farther apart than their width.")}</p>
{#if bad}<p role="alert">{t("Bridge {0} is invalid: {1}",bad,error ?? '')}</p>{/if}
<ol class="bridge-list">{#each bridges.connections as connection,i}<li><strong>{t("Bridge {0}",i+1)}</strong> · {connection.first.point.x.toFixed(2)}, {connection.first.point.y.toFixed(2)} → {connection.second.point.x.toFixed(2)}, {connection.second.point.y.toFixed(2)}<button class="btn btn-ghost bridge-remove" disabled={removing || !!ui.picking} onclick={() => remove(i)} aria-label={t('Remove bridge {0}', i + 1)}>{t('Remove')}</button></li>{/each}</ol>
<Stepper label={t("Channel width")} value={bridges.width} unit="mm" step={0.5} disabled={!on} apply={setWidth} />
{#if ui.picking?.feature === 'bridges'}
<p role="status">{ui.picking.previewPending ? t('Checking the bridge…') : ui.picking.first ? t('Click the second contour · snaps to the line') : t('Click the first contour of the next bridge')}</p>
{#if ui.picking.error}<p role="alert">{ui.picking.error}</p>{/if}
<button class="btn btn-ghost" onclick={()=>{if(ui.picking)ui.picking={...ui.picking,first:null,error:null};}}>{t('Take back the first point')}</button>
<button class="btn btn-soft" onclick={()=>ui.picking=null}>{t('End bridge mode')}</button>
{/if}
<button class="btn btn-primary" onclick={() => pick('bridges')}>{t("Add bridge")}</button>
<Placed count={bridges.connections.length} what="bridge" disabled={!on} onclear={clear} onpick={() => pick('bridges')} />
<div class="param">
  <div class="lbl">{t("Tap two contours to join, or one contour twice to split.")}</div>
  <button class="btn btn-ghost" onclick={undoLast} disabled={!on || bridges.connections.length === 0}>{t("Undo last")}</button>
</div>
