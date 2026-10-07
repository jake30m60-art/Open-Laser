<script lang="ts">
  import { t } from '../i18n/index';

  // A picture to engrave: how wide it lies on the sheet, the height
  // following its proportions, and how many pixels that keeps. The browser
  // decodes the file and weighs its colours to gray; the server keeps the
  // gray pixels as an image part on the layer Image.
  import Modal from './Modal.svelte';
  import Stepper from './params/Stepper.svelte';
  import { api } from '../api/client';
  import { ui } from '../stores/ui.svelte';
  import { explain } from '../lib/format';
  import { grayPixels, heightFor, targetSize } from '../lib/image-import';

  let { file, onclose }: { file: File; onclose: () => void } = $props();

  let bitmap = $state<ImageBitmap | null>(null);
  let widthMm = $state(100);
  let busy = $state(false), error = $state('');
  let preview = $state<HTMLCanvasElement>();

  const heightMm = $derived(bitmap ? heightFor(bitmap.width, bitmap.height, widthMm) : 0);
  const kept = $derived(bitmap ? targetSize(bitmap.width, bitmap.height, widthMm) : null);
  const name = $derived(file.name.replace(/\.[^.]+$/, '') || 'Image');

  $effect(() => {
    let live = true;
    createImageBitmap(file)
      .then((decoded) => { if (live) bitmap = decoded; else decoded.close(); })
      .catch(() => { if (live) error = t('This file is not a picture the browser can open.'); });
    return () => { live = false; };
  });

  // The preview shows the picture in the gray it engraves.
  $effect(() => {
    if (!bitmap || !preview) return;
    const scale = Math.min(1, 320 / bitmap.width, 220 / bitmap.height);
    preview.width = Math.max(1, Math.round(bitmap.width * scale));
    preview.height = Math.max(1, Math.round(bitmap.height * scale));
    const context = preview.getContext('2d');
    if (!context) return;
    context.drawImage(bitmap, 0, 0, preview.width, preview.height);
    const data = context.getImageData(0, 0, preview.width, preview.height);
    const gray = grayPixels(data.data);
    for (let i = 0; i < gray.length; i++) data.data.set([gray[i]!, gray[i]!, gray[i]!, 255], i * 4);
    context.putImageData(data, 0, 0);
  });

  async function importImage(): Promise<void> {
    if (!bitmap || !kept || busy) return;
    busy = true; error = '';
    try {
      const canvas = document.createElement('canvas');
      canvas.width = kept.width;
      canvas.height = kept.height;
      const context = canvas.getContext('2d');
      if (!context) throw new Error(t('The browser cannot read the picture.'));
      context.imageSmoothingQuality = 'high';
      context.drawImage(bitmap, 0, 0, kept.width, kept.height);
      const pixels = grayPixels(context.getImageData(0, 0, kept.width, kept.height).data);
      const { id } = await api.importImage(file.name, { ...kept, widthMm, heightMm }, pixels);
      ui.selected = id;
      ui.say(t('Imported {0} as an engraving, {1} × {2} mm', name, widthMm.toFixed(1), heightMm.toFixed(1)));
      onclose();
    } catch (e) {
      error = explain(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t('Import a picture')} onclose={() => { if (!busy) onclose(); }}>
  <p class="muted">{t('The picture is engraved on the layer Image, line by line: dark parts with more power, light parts with less. Set the size it has on the sheet.')}</p>
  <div class="image-preview">{#if bitmap}<canvas bind:this={preview}></canvas>{:else if !error}<span class="muted">{t('Opening…')}</span>{/if}</div>
  {#if bitmap}
    <Stepper label={t('Width')} value={widthMm} unit="mm" step={5} apply={(v) => (widthMm = Math.min(2000, Math.max(1, v)))} />
    <p class="facts">{t('Height')}: {heightMm.toFixed(1)} mm · {t('Pixels kept')}: {kept?.width} × {kept?.height} {#if kept && kept.width < bitmap.width}<small>({t('from {0} × {1}', bitmap.width, bitmap.height)})</small>{/if}</p>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="actions">
    <button class="btn btn-ghost lg" disabled={busy} onclick={onclose}>{t('Cancel')}</button>
    <button class="btn btn-primary lg" disabled={busy || !bitmap} onclick={importImage}>{busy ? t('Importing…') : t('Import')}</button>
  </div>
</Modal>

<style>
  .muted { margin: 0 0 12px; line-height: 1.6; color: var(--ink-3); }
  .image-preview { display: grid; place-items: center; min-height: 120px; background: var(--panel-2); border: 1px solid var(--line); border-radius: 10px; padding: 8px; margin-bottom: 12px; }
  .image-preview canvas { max-width: 100%; image-rendering: auto; }
  .facts { margin: 8px 0; font-size: var(--t-sm); color: var(--ink-2); }
  .facts small { color: var(--ink-3); }
  .error { color: var(--warn); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
</style>
