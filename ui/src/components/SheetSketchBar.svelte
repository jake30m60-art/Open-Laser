<script lang="ts">
  // While a special sheet is sketched for the open job: one click saves the
  // outline as a remnant and makes it the job's sheet.
  import { t } from '../i18n/index';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { explain } from '../lib/format';
  import { outerFirst } from '../lib/workpiece-alignment';
  import { sketchStore } from '../sketch/store.svelte';
  import { toDxf } from '../sketch/dxf';
  import type { RecipeView } from '../api';
  let busy = $state(false);
  async function take(): Promise<void> {
    busy = true;
    try {
      const drawing = outerFirst(await api.alignmentSketch(toDxf(sketchStore.sketch)));
      if (!drawing.contours.length) { ui.say(t('Draw the outline of the sheet first.'), true); return; }
      // The sheet is of what the job cuts: the job's material, or a cut layer's own recipe.
      const draft = server.doc?.draft;
      const cuts = (r: RecipeView | null | undefined): r is RecipeView => !!r && r.attributes['OpenLaserProcess'] !== 'engrave' && r.thickness_mm > 0;
      const recipe = [draft?.recipe, ...(draft?.layers ?? []).filter((l) => l.mode !== 'mark').map((l) => l.recipe)].find(cuts) ?? draft?.recipe;
      const { id } = await api.saveSketchRemnant({
        id: null, revision: null, name: sketchStore.name || t('Special sheet'),
        material: recipe?.name ?? t('Sheet'), thickness_mm: recipe?.thickness_mm || 1,
        mode: recipe?.laser ?? server.doc?.mode ?? 'fiber', drawing, inspected: true,
      });
      await api.setStock({ kind: 'remnant', id });
      ui.sheetSketch = false;
      ui.tab = 'setup';
      ui.setupPanel = 'sheet';
      ui.say(t('Special sheet saved and used for this job.'));
    } catch (e) { ui.say(explain(e), true); }
    finally { busy = false; }
  }
</script>

<div class="sheet-sketch-bar" role="region" aria-label={t('Special sheet')}>
  <span><strong>{t('Special sheet')}</strong> · {t('Draw the outline of the sheet and any holes, then take it as the job\'s sheet.')}</span>
  <button class="btn btn-ghost" disabled={busy} onclick={() => { ui.sheetSketch = false; ui.tab = 'setup'; }}>{t('Cancel')}</button>
  <button class="btn btn-primary" disabled={busy || !server.doc?.draft} onclick={take}>{t('Use as sheet')}</button>
</div>

<style>
  .sheet-sketch-bar { display:flex; align-items:center; gap:10px; flex-wrap:wrap; padding:8px 12px; border:1px solid var(--accent); border-radius:12px; background:var(--panel-2); margin-bottom:8px; }
  .sheet-sketch-bar span { flex:1; min-width:220px; }
</style>
