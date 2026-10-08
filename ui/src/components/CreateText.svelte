<script lang="ts">
  import { t } from '../i18n/index';

  import { displayNumber, quantity, unitLabel } from '../lib/units.svelte';
  import Modal from './Modal.svelte';
  import FontPicker from './FontPicker.svelte';
  import { api, type TextOptions, type TextPreview } from '../api/client';
  import { ui } from '../stores/ui.svelte';
  import { server } from '../stores/server.svelte';
  import { osk } from '../lib/osk.svelte';
  import { explain, plural } from '../lib/format';
  import { boxOf, pathOf, viewBoxFor } from '../lib/svg';
  import { untrack } from 'svelte';
  import type { SketchText } from '../sketch/model';

  let { onclose, oncreated, editing = null, onplace }: {
    onclose: () => void;
    oncreated?: (id:string)=>Promise<void>;
    /** A sketch's text to change, prefilling the form. */
    editing?: SketchText | null;
    /** In a sketch: the text and its outlines go back as an object, not a part. */
    onplace?: (text: Omit<SketchText, 'id' | 'x' | 'y'>) => void;
  } = $props();
  // The form starts from the text being changed, once.
  const start = untrack(() => editing);
  let value = $state(start?.value ?? 'OpenLaser');
  let family = $state<TextOptions['family']>(start?.family ?? 'sans');
  let font = $state<string | undefined>(start?.font);
  let importing = $state(false);
  let fontRevision = $state(0);
  let size = $state(start?.size ?? 20);
  let bold = $state(start?.bold ?? false);
  let alignment = $state<TextOptions['alignment']>(start?.alignment ?? 'left');
  let preview = $state<TextPreview | null>(null);
  let pending = $state(false);
  let creating = $state(false);
  let error = $state('');
  const options = $derived<TextOptions>({ value, family, size, bold, alignment, font });
  const box = $derived(preview ? boxOf(preview.outline) : null);

  $effect(() => {
    const next = options;
    void fontRevision;
    let cancelled = false;
    preview = null;
    error = '';
    const hasText = !!next.value.trim();
    pending = hasText;
    if (!hasText) return;
    const timer = setTimeout(async () => {
      try { const result = await api.previewText(next); if (!cancelled) preview = result; }
      catch (reason) { if (!cancelled) error = explain(reason); }
      finally { if (!cancelled) pending = false; }
    }, 180);
    return () => { cancelled = true; clearTimeout(timer); };
  });

  function close(): void { if (!creating && !importing) { osk.close(); onclose(); } }

  /** The text as a sketch object: outlines from their box's lower left corner. */
  function place(): void {
    if (!preview || !box || !onplace) return;
    const outline = preview.outline.map((line) => line.map(([x, y]) => [x! - box.minX, y! - box.minY] as [number, number]));
    onplace({ value, family, font, size, bold, alignment, outline, width: box.maxX - box.minX, height: box.maxY - box.minY });
    osk.close();
    onclose();
  }

  async function create(): Promise<void> {
    if (onplace) { place(); return; }
    if (creating || importing || pending || !preview) return;
    creating = true;
    error = '';
    osk.close();
    try {
      const name = value.trim().split(/\r?\n/)[0]!.replace(/[\\/]/g, ' ').slice(0, 80);
      const { id } = await api.createText(name || 'Text', options);
      if(oncreated){await oncreated(id);onclose();return;}
      ui.selected = id;
      // With a job open the text joins it beside the sheet; otherwise it opens as the job.
      if (server.doc?.draft) {
        await api.addParts([id]);
        ui.tab = 'setup';
        onclose();
        ui.say('Text added beside the sheet · Undo takes it off');
      } else {
        await api.openPart(id);
        ui.tab = 'setup';
        onclose();
        ui.say('Text added. Choose a material to prepare it for cutting.');
      }
    } catch (reason) { error = explain(reason); }
    finally { creating = false; }
  }
</script>

<Modal title={t("Add text")} wide onclose={close}>
  <div class="text-editor">
    <label class="text-label">{t("Text")}<textarea aria-label={t("Text to cut")} rows="4" bind:value disabled={creating} placeholder={t("Type or paste your text…")}></textarea></label>
    <div class="text-controls">
      <div class="text-fonts"><FontPicker bind:family bind:font bind:importing disabled={creating} onimport={() => fontRevision++} /></div>
      <div class="text-size-row"><span>{t("Font size")}</span><button
        class="text-number" data-numpad aria-label={t("Font size")} disabled={creating}
        onclick={() => osk.number(t('Font size'), size, 'mm', (next) => (size = next))}>{displayNumber(size, 'mm')} <small>{unitLabel('mm')}</small></button></div>
      {#if !font}<div class="text-choices text-weight">
        <button class:on={!bold} aria-pressed={!bold} disabled={creating} onclick={() => (bold = false)}>{t("Regular")}</button>
        <button class:on={bold} aria-pressed={bold} disabled={creating} onclick={() => (bold = true)}>{t("Bold")}</button>
      </div>{:else}<p class="muted text-face-note">{t("Weight and style come from the selected font.")}</p>{/if}
      <div><div class="text-label">{t("Align lines")}</div><div class="text-choices">
        {#each ['left', 'center', 'right'] as side}
          <button class:on={alignment === side} aria-pressed={alignment === side} disabled={creating} onclick={() => (alignment = side as TextOptions['alignment'])}>{t(side === 'left' ? 'Left' : side === 'right' ? 'Right' : 'Center')}</button>
        {/each}
      </div></div>
    </div>
    <div class="text-preview" aria-label={t("Text outline preview")} aria-busy={pending}>
      {#if preview && box}
        <svg viewBox={viewBoxFor(box, 600, 190, 0.1)} role="img" aria-label={t("Cuttable text outlines")}><path d={preview.outline.map((line) => pathOf(line)).join(' ')} fill-rule="evenodd" vector-effect="non-scaling-stroke" /></svg>
        <div class="text-dimensions">{displayNumber(preview.width, 'mm', 1)} × {quantity(preview.height, 'mm', 1)} · {plural(preview.contours, 'outline')}</div>
      {:else}<span class="muted">{pending ? t('Preparing text…') : t('Your text preview appears here.')}</span>{/if}
    </div>
    {#if error}<p class="warn-text" role="alert">{error}</p>{/if}
    {#each preview?.warnings ?? [] as warning}<p class="warn-text" role="status">{warning}</p>{/each}
    <div class="text-footer"><span class="muted">{t("Overlapping letters are welded into cutting outlines.")}</span><button
      class="btn btn-primary" disabled={creating || importing || pending || !preview} onclick={create}>{creating ? t("Adding…") : editing ? t("Change text") : onplace ? t("Place in sketch") : t("Add text")}</button></div>
  </div>
</Modal>

<style>
  .text-editor { display: grid; grid-template-columns:minmax(0,1fr); min-width:0; gap:18px; }
  .text-label { display: grid; gap: 8px; font-size: var(--t-sm); color: var(--ink-2); }
  textarea { width: 100%; resize: vertical; min-height: 100px; border: 1px solid var(--line); border-radius: 10px; background: var(--panel-2); color: var(--ink); padding: 12px; font: inherit; font-size: var(--t-lg); line-height: 1.4; }
  .text-controls { display: grid; grid-template-columns: 1.3fr 1fr; gap: 14px 24px; }
  .text-fonts { grid-column: 1 / -1; }
  .text-face-note { align-self: center; font-size: var(--t-sm); margin: 0; }
  .text-choices { display: flex; gap: 6px; }
  .text-choices button, .text-number { min-height: 44px; border: 1px solid var(--line); border-radius: 8px; background: var(--panel-2); color: var(--ink); font: inherit; cursor: pointer; padding: 10px 15px; }
  .text-choices button { flex: 1; }
  .text-choices .on { color: var(--accent); border-color: var(--accent); background: color-mix(in srgb, var(--accent) 9%, var(--panel)); }
  .text-size-row { display: flex; align-items: end; justify-content: space-between; gap: 10px; padding-bottom: 1px; font-size: var(--t-sm); }
  .text-size-row > span { align-self: center; }
  .text-weight { align-self: end; }
  .text-preview { min-height: 220px; display: grid; place-items: center; gap: 10px; padding: 16px; border: 1px solid var(--line); border-radius: 12px; background: var(--panel-2); }
  .text-preview svg { width: 100%; height: 190px; color: var(--accent); }
  .text-preview path { fill: color-mix(in srgb, var(--accent) 14%, transparent); stroke: currentColor; stroke-width: 1; }
  .text-dimensions { font-size: var(--t-sm); color: var(--ink-2); }
  .text-footer { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .text-footer span { font-size: var(--t-sm); }
  @media (max-width: 650px) { .text-controls { grid-template-columns: 1fr; } .text-footer { flex-wrap: wrap; } }
</style>
