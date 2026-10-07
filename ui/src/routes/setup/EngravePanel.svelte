<script lang="ts">
  // Engraving, in one place on the Setup page: shapes picked on the drawing
  // go to an engraving layer; each engraving layer has its recipe and its
  // scan fill, shows or hides on the drawing, and shows its run-ups and
  // what it engraves when paths are on.
  import { t as tr } from '../../i18n/index';
  import { api } from '../../api/client';
  import { server } from '../../stores/server.svelte';
  import { ui } from '../../stores/ui.svelte';
  import { featureEdits } from '../../stores/feature-edits';
  import { scanOffsets } from '../../stores/scan-offsets.svelte';
  import { explain } from '../../lib/format';
  import { defaultFill, setFill, swatchColor } from '../../lib/drawing-layers';
  import { fillOf, isEngraving, offsetAt } from '../../lib/engraving';
  import FillSettings from './FillSettings.svelte';
  import type { DraftLayer, Fill, LayerChange, RecipeView } from '../../api';

  let { selectedContours }: { selectedContours: number[] } = $props();
  const draft = $derived(server.doc!.draft!);
  const NAME = 'Engraving';
  const order = $derived(draft.layers.map((l) => l.name));
  const marked = $derived(draft.layers.filter((l) => l.mode === 'mark'));
  const cutting = $derived(draft.layers.filter((l) => l.mode !== 'mark'));
  const laser = $derived(draft.recipe?.laser ?? server.doc?.mode ?? 'fiber');
  const recipes = $derived((server.doc?.library.recipes ?? []).filter((r) => r.laser === laser && isEngraving(r)));
  /** Every other recipe for the laser, offered when there is no engraving recipe. */
  const others = $derived((server.doc?.library.recipes ?? []).filter((r) => r.laser === laser && !isEngraving(r)));
  const fills = $derived(featureEdits.value(draft).layers ?? []);
  const fillOfLayer = (name: string): Fill | null => fills.find((l) => l.name === name)?.machining?.fill ?? null;
  let busy = $state(false);

  async function change(...edits: LayerChange[]): Promise<void> {
    busy = true;
    try { for (const edit of edits) await api.changeLayers(edit); }
    catch (error) { ui.say(explain(error), true); throw error; }
    finally { busy = false; }
  }
  function writeFill(layer: string, next: Fill | null): void {
    busy = true;
    featureEdits.change((f) => setFill(f, layer, next, order))
      .then(() => ui.say(tr("{0}: {1}. Preview updated; Undo restores the previous setting.", layer, tr(!next ? "Contours only" : next.outline ? "Fill and contours" : "Fill area"))))
      .catch((error) => ui.say(explain(error), true))
      .finally(() => { busy = false; });
  }
  /** A layer engraves: marked, filled, with the first engraving recipe when it has none. */
  async function engrave(layer: string, recipe: RecipeView | null = recipes[0] ?? null): Promise<void> {
    const edits: LayerChange[] = [{ kind: 'mode', layer, mode: 'mark' }, { kind: 'output', layer, on: true }];
    const current = draft.layers.find((l) => l.name === layer);
    if (recipe && !current?.recipe) edits.push({ kind: 'recipe', layer, recipe: recipe.id });
    await change(...edits);
    if (current?.mode !== 'mark' && !fillOfLayer(layer)) writeFill(layer, recipe ? fillOf(recipe.attributes) : defaultFill());
  }
  /** The selected shapes onto the engraving layer, which then engraves. */
  async function engraveSelection(): Promise<void> {
    const contours = [...selectedContours];
    if (!contours.length) return;
    try {
      await change({ kind: 'assign', contours, layer: NAME });
      await engrave(NAME);
      ui.say(tr("{0} shapes engrave on {1} · Undo moves them back", contours.length, NAME));
    } catch { /* said */ }
  }
  async function useRecipe(layer: DraftLayer, recipe: RecipeView): Promise<void> {
    try {
      await change({ kind: 'recipe', layer: layer.name, recipe: recipe.id });
      const currentFill = fillOfLayer(layer.name);
      if (currentFill) writeFill(layer.name, { ...fillOf(recipe.attributes), outline: currentFill.outline });
    } catch { /* said */ }
  }
  const backToCut = (layer: string) => change({ kind: 'mode', layer, mode: 'cut' }).catch(() => undefined);
  const speedOf = (layer: DraftLayer): number | null => {
    const v = Number((layer.recipe ?? draft.recipe)?.attributes['CutSpeed']);
    return Number.isFinite(v) && v > 0 ? v : null;
  };
  const offsetOf = (layer: DraftLayer, fill: Fill): number | null => {
    const speed = speedOf(layer);
    return speed ? offsetAt(scanOffsets.entries(laser), speed, fill.angle) : null;
  };
  /** What a layer fills, as the preview prepared it: closed outlines or the fill's own lines. */
  const closedOn = (name: string): number => (draft.preview?.contours ?? []).filter((c) => c.layer === name && (c.closed || c.paths.some((p) => p.kind === 'travel'))).length;
  /** The layer each placed contour is drawn on, as the preview has it. */
  const layerOf = $derived(new Map([...(draft.preview?.contours ?? []), ...(draft.preview?.engraving_regions ?? [])].filter((c) => c.sources.length === 1).map((c) => [c.sources[0]!, c.layer])));
  /** Selected shapes on an engraving layer. */
  const engravedSelection = $derived(selectedContours.filter((i) => marked.some((l) => l.name === layerOf.get(i))));
  async function cutOn(layer: string): Promise<void> {
    const contours = [...engravedSelection];
    try {
      await change({ kind: 'assign', contours, layer });
      ui.say(tr("{0} shapes are cut on {1} again · Undo takes it back", contours.length, layer));
    } catch { /* said */ }
  }
  const close = () => (ui.setupPanel = null);
</script>

<div class="feat-head">
  <h2 class="feat-title"><i class="ic ic-tool-engrave"></i>{tr("Engrave")}</h2>
  <button class="btn btn-ghost" onclick={close}>{tr("Close")}</button>
</div>
<p class="feat-desc">{tr("Engraving marks the surface line by line. Pick the shapes to engrave on the drawing, then set the lines here.")}</p>

<div class="stack">
  {#if busy || draft.error === 'preparing geometry'}<p role="status">{tr("Updating engraving preview…")}</p>{/if}
  <p class="muted">{tr("Selected now: {0} contours", selectedContours.length)}<br />{tr("Click a contour to select only it. Shift-click adds or removes contours.")}</p>
  <button class="btn btn-primary block" disabled={busy || !selectedContours.length} onclick={engraveSelection}>
    {selectedContours.length ? tr("Engrave the selection ({0} shapes)", selectedContours.length) : tr("Select shapes to engrave")}
  </button>
  {#if engravedSelection.length && cutting.length}
    <div class="back">
      <span>{tr("Stop engraving the selected shapes ({0}), cut them on:", engravedSelection.length)}</span>
      <div class="recipes">
        {#each cutting as layer (layer.name)}
          <button class="chip" disabled={busy} onclick={() => cutOn(layer.name)}>
            <span class="layer-swatch" style:--layer={swatchColor(draft.layers, layer.name)}></span>{layer.name}
          </button>
        {/each}
      </div>
    </div>
  {/if}
  <div class="paths">
    <span><strong>{tr("Show tool paths")}</strong><small>{tr("Run-ups and turns dashed, engraved stretches solid")}</small></span>
    <button class="switch" class:on={ui.layerShown('travel')} aria-label={tr("Show tool paths")} onclick={() => ui.toggleLayer('travel')}></button>
  </div>

  {#if draft.error && draft.error !== 'preparing geometry'}
    <p class="warn-box" role="alert">{tr("The hatching cannot be shown: {0}", draft.error)}<br />{tr("The complete job could not be prepared. Valid engraving layers are previewed separately; fix the geometry error before running.")}</p>
  {/if}
  {#if !draft.recipe}
    <p class="warn-box">{tr("No job recipe selected. Layers with their own recipe can still engrave; choose a recipe for the other layers before running.")}</p>
  {/if}
  {#each marked as layer (layer.name)}
    {@const fill = fillOfLayer(layer.name)}
    {@const closed = closedOn(layer.name)}
    {@const hidden = ui.drawingLayerHidden(layer.name)}
    <section class="layer" class:hidden>
      <header>
        <span class="layer-swatch mark" style:--layer={swatchColor(draft.layers, layer.name)}></span>
        <strong>{layer.name}</strong>
        <small>{tr("On this layer: {0} contours", layer.contours)}</small>
        <button class="icon-btn" aria-label={hidden ? tr("Show on the drawing") : tr("Hide on the drawing")} onclick={() => ui.toggleDrawingLayer(layer.name)}>
          <i class="ic {hidden ? 'ic-eye-off' : 'ic-eye'}"></i>
        </button>
      </header>
      <div class="recipes" role="group" aria-label={tr("Engraving mode")}>
        <button class="chip" class:on={!fill} aria-pressed={!fill} disabled={busy} onclick={() => writeFill(layer.name, null)}>{tr("Contours only")}</button>
        <button class="chip" class:on={!!fill && !fill.outline} aria-pressed={!!fill && !fill.outline} disabled={busy} onclick={() => writeFill(layer.name, { ...(fill ?? defaultFill()), outline: false })}>{tr("Fill area")}</button>
        <button class="chip" class:on={!!fill && fill.outline} aria-pressed={!!fill && fill.outline} disabled={busy} onclick={() => writeFill(layer.name, { ...(fill ?? defaultFill()), outline: true })}>{tr("Fill and contours")}</button>
      </div>
      <p class="muted">{tr("Applies to every shape on this layer. Contours only traces the selected outlines; fill scans closed areas and leaves holes clear.")}</p>
      {#if !layer.chosen}
        <p class="warn-box">{tr("This layer has no recipe yet, so nothing on it is worked out or engraved. Choose one:")}</p>
      {/if}
      <details class="recipe-picker">
        <summary>{tr("Recipe")}: {(layer.recipe ?? draft.recipe)?.name ?? tr("Choose one")}</summary>
      <div class="recipes" role="group" aria-label={tr("Engraving recipes")}>
        {#each recipes as r (r.id)}
          <button class="chip" class:on={layer.recipe?.id === r.id} disabled={busy} onclick={() => useRecipe(layer, r)}>{r.name} · {r.gas}</button>
        {:else}
          <p class="muted">{tr("No engraving recipe yet: in Materials, open a recipe and choose Make it an engraving recipe.")}</p>
          {#each others as r (r.id)}
            <button class="chip" class:on={layer.recipe?.id === r.id} disabled={busy} onclick={() => useRecipe(layer, r)}>{r.name} · {r.thickness_mm} mm · {r.gas}</button>
          {/each}
        {/each}
        <button class="link" onclick={() => (ui.layerSheet = layer.name)}>{tr("All layer settings")}</button>
      </div>
      </details>
      {#if fill && draft.recipe && !draft.error && closed === 0}
        <p class="warn-box">{tr("No closed shape on this layer: a fill engraves inside closed outlines only. Close the outlines in the sketch, or the lines are traced as they are.")}</p>
      {/if}
      {#if fill}
        <FillSettings {fill} apply={(c) => writeFill(layer.name, { ...fill, ...c })} speed={speedOf(layer)} offset={offsetOf(layer, fill)} />

      {/if}
      <button class="link" disabled={busy} onclick={() => backToCut(layer.name)}>{tr("Cut this layer instead")}</button>
    </section>
  {/each}

  {#if cutting.length}
    <h3 class="other">{tr("Engrave a whole layer")}</h3>
    <div class="recipes">
      {#each cutting as layer (layer.name)}
        <button class="chip" disabled={busy} onclick={() => engrave(layer.name).catch(() => undefined)}>
          <span class="layer-swatch" style:--layer={swatchColor(draft.layers, layer.name)}></span>{layer.name}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .stack > .btn { white-space:normal; height:auto; min-height:48px; }
  .recipe-picker summary { cursor:pointer; padding:10px 0; overflow-wrap:anywhere; }
  .paths { display:flex; align-items:center; justify-content:space-between; gap:12px; padding:6px 0; }
  .paths small { display:block; font-size:var(--t-sm); color:var(--ink-3); }
  .layer { border:1px solid var(--line); border-radius:12px; padding:10px 12px; display:grid; gap:8px; }
  .layer.hidden { opacity:.6; }
  .layer header { display:flex; align-items:center; gap:8px; }
  .layer header strong { flex:1; }
  .layer header small { color:var(--ink-3); font-size:var(--t-sm); }
  .icon-btn { width:44px; height:44px; display:grid; place-items:center; border:1px solid var(--line); border-radius:10px; background:var(--panel-2); color:var(--ink-2); cursor:pointer; }
  .recipes { display:flex; flex-wrap:wrap; gap:6px; align-items:center; }
  .recipes .chip { min-height:44px; }
  .back { display:grid; gap:6px; padding:8px 10px; border:1px dashed var(--line); border-radius:10px; font-size:var(--t-sm); }
  .warn-box { margin:0; padding:10px 12px; border-radius:10px; background:var(--warn-soft); color:var(--warn); font-size:var(--t-sm); }
  .layer-swatch { flex:none; width:14px; height:14px; border-radius:4px; background:var(--layer, var(--cut)); }
  .layer-swatch.mark { background:transparent; border:2px dashed var(--layer, var(--cut)); }
  .other { margin:8px 0 0; font-size:var(--t-sm); color:var(--ink-2); }
</style>
