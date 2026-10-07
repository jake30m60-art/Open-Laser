<script lang="ts">
  import { api } from '../../api/client';
  import { explain } from '../../lib/format';
  import { server } from '../../stores/server.svelte';
  import { t } from '../../i18n/index';

  // While a feature asks for places on the drawing: what to tap, and the
  // way out. The canvas takes the taps.
  import { ui, type Picking } from '../../stores/ui.svelte';

  let { picking, picked, total, busy, onfinish }: {
    picking: Picking;
    /** Groups tapped so far while picking a cutting order, of `total`. */
    picked: number;
    total: number;
    busy: boolean;
    onfinish: () => void;
  } = $props();

  const hint = $derived.by(() => {
    switch (picking.feature) {
      case 'joints': return 'Tap a contour where a joint should hold it';
      case 'cooling': return 'Tap a contour where the cut should pause to cool';
      case 'start': return 'Tap a contour where its cut should start';
      case 'bridges': return picking.bridgeReview ? picking.error ? 'Invalid bridge. Undo the last bridge or cancel.' : 'Review the bridge preview, then click the drawing to confirm.' : picking.first ? 'Tap the other end of the bridge' : 'Tap the first end of the bridge';
      case 'order': return `Tap the contours in cutting order · ${picked} of ${total}`;
    }
  });
  const draftError = $derived(server.doc?.draft?.error);
  function cancelBridgeEnd(): void {
    if (ui.picking) ui.picking = { ...ui.picking, first: null };
  }
  async function undoBridge(): Promise<void> {
    const current=ui.picking; if(!current?.features.bridges || busy)return;
    const features=structuredClone($state.snapshot(current.features)); features.bridges!.connections.pop();
    const staged={...current,features,bridgeReview:false,first:null,marks:current.marks.slice(0,-1),preview:undefined,error:null,previewPending:true}; ui.picking=staged;
    try{const reply=await api.previewFeatures(features,current.revision); if(ui.picking===staged)ui.picking={...staged,preview:reply.preview,error:reply.error,previewPending:false};}catch(e){ui.say(explain(e),true);}
  }
  function cancel(): void {
    ui.picking = null;
  }
</script>

<div class="pick-bar">
  <span>{t(hint)}</span>
  {#if picking.error && picking.error !== draftError}<span role="alert">{picking.error}</span>{/if}
  {#if picking.feature === 'bridges' && picking.first}
    <button class="btn btn-ghost" onclick={cancelBridgeEnd}>{t("Cancel end")}</button>
  {/if}
  {#if picking.feature === 'bridges' && picking.features.bridges?.connections.length}<button class="btn btn-ghost" disabled={busy} onclick={undoBridge}>{t("Undo last bridge")}</button>{/if}
  <button class="btn btn-ghost" onclick={cancel}>{t("Cancel")}</button>
  {#if picking.feature !== 'bridges'}<button class="btn btn-primary" disabled={busy || picking.previewPending || !!picking.first || !!picking.error && picking.error !== draftError} onclick={onfinish}>{t("Apply")}</button>{/if}
</div>
