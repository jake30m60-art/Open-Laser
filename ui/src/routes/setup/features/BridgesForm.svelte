<script lang="ts">
  import { ui } from '../../../stores/ui.svelte';
  import { server } from '../../../stores/server.svelte';
  import { t } from '../../../i18n/index';

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
  const removeFrom = (index:number) => set(f=>{if(f.bridges)f.bridges.connections=f.bridges.connections.slice(0,index);});
  const setWidth = (v: number) => set((f) => { f.bridges!.width = v; });
  const clear = () => set((f) => { f.bridges!.connections = []; });
  const undoLast = () => set((f) => { f.bridges!.connections.pop(); });
</script>

<p class="muted">{t("Click the first edge to open its preview. Snap to the second edge and click to save. Continue with the next bridge; Undo restores the previous cut.")}</p>
<p class="muted">{t("Stencil bridge: connect the outer boundary to the inner hole. Keep separate bridges farther apart than their width.")}</p>
{#if bad}<p role="alert">{t("Bridge {0} is invalid: {1}",bad,error ?? '')}</p>{/if}
<ol class="bridge-list">{#each bridges.connections as connection,i}<li><strong>{t("Bridge {0}",i+1)}</strong> · {connection.first.point.x.toFixed(2)}, {connection.first.point.y.toFixed(2)} → {connection.second.point.x.toFixed(2)}, {connection.second.point.y.toFixed(2)}<button class="btn btn-ghost" onclick={()=>removeFrom(i)}>{t("Remove this and subsequent bridges")}</button></li>{/each}</ol>
<Stepper label={t("Channel width")} value={bridges.width} unit="mm" step={0.5} disabled={!on} apply={setWidth} />
{#if ui.picking?.feature === 'bridges'}
<p role="status">{ui.picking.previewPending ? 'Brücke wird geprüft…' : ui.picking.first ? 'Zweite Kontur anklicken · rastet an der Linie ein' : 'Erste Kontur für die nächste Brücke anklicken'}</p>
{#if ui.picking.error}<p role="alert">{ui.picking.error}</p>{/if}
<button class="btn btn-ghost" onclick={()=>{if(ui.picking)ui.picking={...ui.picking,first:null,error:null};}}>Ersten Punkt zurücknehmen</button>
<button class="btn btn-soft" onclick={()=>ui.picking=null}>Brückenmodus beenden</button>
{/if}
<button class="btn btn-primary" onclick={() => pick('bridges')}>{t("Add bridge")}</button>
<Placed count={bridges.connections.length} what="bridge" disabled={!on} onclear={clear} onpick={() => pick('bridges')} />
<div class="param">
  <div class="lbl">{t("Tap two contours to join, or one contour twice to split.")}</div>
  <button class="btn btn-ghost" onclick={undoLast} disabled={!on || bridges.connections.length === 0}>{t("Undo last")}</button>
</div>
