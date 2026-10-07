<script lang="ts">
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
      case 'bridges': return picking.first ? 'Tap the other end of the bridge' : 'Tap the first end of the bridge';
      case 'order': return `Tap the contours in cutting order · ${picked} of ${total}`;
    }
  });
  const draftError = $derived(server.doc?.draft?.error);
  function cancelBridgeEnd(): void {
    if (ui.picking) ui.picking = { ...ui.picking, first: null };
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
  <button class="btn btn-ghost" onclick={cancel}>{t("Cancel")}</button>
  <button class="btn btn-primary" disabled={busy || !!picking.first || !!picking.error && picking.error !== draftError} onclick={onfinish}>{t("Apply")}</button>
</div>
