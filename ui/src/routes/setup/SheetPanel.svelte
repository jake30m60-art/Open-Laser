<script lang="ts">
  // The sheet the job is cut from: its size, turned a quarter, moved under
  // the parts by dragging it on the drawing. Its front-left corner is what
  // the origin stands for unless a point was picked in the drawing.
  import { t } from '../../i18n/index';
  import SheetSizePicker from '../../components/SheetSizePicker.svelte';
  import HoldButton from '../../components/HoldButton.svelte';
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
  const corner = $derived(draft.sheet_corner);
  const head = $derived(server.doc!.machine.feedback?.position_mm);
  /** The sheet's origin at the head: the sheet lies axis-parallel from there. */
  async function setOrigin(at?: 'clear'): Promise<void> {
    busy = true;
    try {
      await api.setSheetOrigin(at);
      ui.say(at ? t('Sheet back at the bed corner') : t('Sheet origin set at the head'));
    } catch (e) { ui.say(explain(e), true); }
    finally { busy = false; }
  }
  const cuts = $derived(server.doc!.sheet_cuts?.length ?? 0);
  async function clearCuts(): Promise<void> {
    busy = true;
    try { await api.clearSheetCuts(); ui.say(t('Cut paths cleared')); }
    catch (e) { ui.say(explain(e), true); }
    finally { busy = false; }
  }
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
<p class="feat-desc">{t("The sheet the job is cut from, with its own origin on the bed, apart from the job origin: move the laser to the sheet's front-left corner and set the sheet origin there.")}</p>

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
    <div class="sheet-origin">
      <span>{t("Sheet origin")}: {#if corner}X {distance(corner[0])} · Y {distance(corner[1])} {unitLabel('mm')}{:else}{t("bed corner")}{/if}</span>
      <HoldButton class="btn btn-move" kind="zero" disabled={busy || !head} onhold={() => setOrigin()}>
        {t("Set sheet origin here")}{#if head} · X {distance(head[0])} · Y {distance(head[1])}{/if}
      </HoldButton>
      {#if corner}<button class="btn btn-ghost" disabled={busy} onclick={() => setOrigin('clear')}>{t("Back to the bed corner")}</button>{/if}
    </div>
    <button class="btn btn-ghost block" disabled={busy || !cuts} onclick={clearCuts}>
      <i class="ic ic-trash"></i>{t("Clear cut parts")}{#if cuts} · {t('{0} cut paths on this sheet', cuts)}{/if}
    </button>
    <div class="row">
      <button class="btn btn-ghost" disabled={busy} onclick={turn}><i class="ic ic-rotate"></i>{t("Turn 90°")}</button>
      <button class="btn btn-ghost" disabled={busy} onclick={remove}><i class="ic ic-trash"></i>{t("Remove sheet")}</button>
    </div>
  {/if}
</div>

<style>
  .stack { display:grid; gap:10px; }
  .row { display:grid; grid-template-columns:1fr 1fr; gap:8px; }
  .sheet-origin { display:grid; gap:6px; padding:8px 10px; border:1px solid var(--line); border-radius:12px; background:var(--panel-2); font-size:var(--t-sm); }
  .sheet-now { margin:4px 0 10px; font-size:var(--t-lg); font-variant-numeric:tabular-nums; }
</style>
