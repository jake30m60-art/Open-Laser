<script lang="ts">
  // The sheet the job is cut from: its size, turned a quarter, moved under
  // the parts by dragging it on the drawing. Its front-left corner is what
  // the origin stands for unless a point was picked in the drawing.
  import { t } from '../../i18n/index';
  import SheetSizePicker from '../../components/SheetSizePicker.svelte';
  import { api } from '../../api/client';
  import { server } from '../../stores/server.svelte';
  import { ui } from '../../stores/ui.svelte';
  import { explain } from '../../lib/format';
  import { distance, unitLabel } from '../../lib/units.svelte';

  const draft = $derived(server.doc!.draft!);
  const stock = $derived(draft.nesting?.stock);
  const rect = $derived(stock?.kind === 'rectangle' ? stock.bounds : null);
  let width = $state(3000);
  let height = $state(1500);
  $effect(() => { if (rect) { width = rect.max.x - rect.min.x; height = rect.max.y - rect.min.y; } });
  let busy = $state(false);

  async function apply(w: number, h: number, min?: [number, number]): Promise<void> {
    busy = true;
    try { await api.setStock({ kind: 'rectangle', width: w, height: h, min }); }
    catch (e) { ui.say(explain(e), true); }
    finally { busy = false; }
  }
  /** The size typed in, keeping the sheet where it lies. */
  const place = () => apply(width, height, rect ? [rect.min.x, rect.min.y] : undefined);
  /** A quarter turn about its front-left corner. */
  const turn = () => { if (rect) void apply(rect.max.y - rect.min.y, rect.max.x - rect.min.x, [rect.min.x, rect.min.y]); };
  async function remove(): Promise<void> {
    busy = true;
    try { await api.setStock({ kind: 'clear' }); ui.say(t('Sheet removed · Undo brings it back')); }
    catch (e) { ui.say(explain(e), true); }
    finally { busy = false; }
  }
</script>

<div class="feat-head">
  <h2 class="feat-title">{t("Sheet")}</h2>
  <button class="btn btn-ghost" onclick={() => (ui.setupPanel = null)}>{t("Close")}</button>
</div>
<p class="feat-desc">{t("The sheet the job is cut from. Drag the sheet to where it lies on the bed: the parts go with it and its corner becomes the absolute origin. Drag parts to move them on the sheet.")}</p>

{#if rect}
  <p class="sheet-now"><strong>{distance(rect.max.x - rect.min.x)} × {distance(rect.max.y - rect.min.y)} {unitLabel('mm')}</strong></p>
{:else if stock}
  <p class="muted">{t("The job uses a remnant or an outline as its sheet.")}</p>
{/if}

<div class="stack">
  <SheetSizePicker bind:width bind:height disabled={busy} />
  <button class="btn btn-primary lg block" disabled={busy || width <= 0 || height <= 0} onclick={place}>
    {rect ? t("Change sheet size") : t("Add sheet")}
  </button>
  {#if rect}
    <div class="row">
      <button class="btn btn-ghost" disabled={busy} onclick={turn}><i class="ic ic-rotate"></i>{t("Turn 90°")}</button>
      <button class="btn btn-ghost" disabled={busy} onclick={remove}><i class="ic ic-trash"></i>{t("Remove sheet")}</button>
    </div>
  {/if}
</div>

<style>
  .stack { display:grid; gap:10px; }
  .row { display:grid; grid-template-columns:1fr 1fr; gap:8px; }
  .sheet-now { margin:4px 0 10px; font-size:var(--t-lg); font-variant-numeric:tabular-nums; }
</style>
