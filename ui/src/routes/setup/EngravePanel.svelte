<script lang="ts">
  import { materialLabel } from '../../lib/materials';
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
  import { fillOf, isEngraving, offsetAt, sameMaterial } from '../../lib/engraving';
  import FillSettings from './FillSettings.svelte';
  import { editPart } from '../../lib/sketch-workflow';
  import { engravingFeatures } from '../../lib/engraving-preview';
  import { imageLayers } from '../../lib/images';
  import type { Features, FillRegion } from '../../api';
  import type { DraftLayer, Fill, LayerChange, RecipeView } from '../../api';

  let { selectedContours, regions, pickMode = $bindable('area'), onclear, onfocus, onremove }: { selectedContours: number[]; regions: FillRegion[]; pickMode?: 'area' | 'contour'; onclear: () => void; onfocus: (source:number) => void; onremove: (source:number) => void } = $props();
  const draft = $derived(server.doc!.draft!);
  /** The layers engraving a picture: they show its shading settings. */
  // While the preview is worked out again it is missing: the picture
  // layers stay as they were, so their settings do not vanish meanwhile.
  let lastPictures = new Set<string>();
  const pictures = $derived.by(() => (draft.preview ? (lastPictures = imageLayers(draft.preview)) : lastPictures));
  const NAME = $derived(ui.engravingLayer ?? 'Engraving');
  const order = $derived(draft.layers.map((l) => l.name));
  const marked = $derived(draft.layers.filter((l) => l.mode === 'mark'));
  const cutting = $derived(draft.layers.filter((l) => l.mode !== 'mark'));
  const laser = $derived(draft.recipe?.laser ?? server.doc?.mode ?? 'fiber');
  // The laser's engraving recipes, the job's material first.
  const recipes = $derived((server.doc?.library.recipes ?? []).filter((r) => r.laser === laser && isEngraving(r))
    .sort((a, b) => Number(sameMaterial(b.name, draft.recipe?.name)) - Number(sameMaterial(a.name, draft.recipe?.name))));
  /** The engraving recipe last chosen for this laser and material on this screen, else the first. */
  const LAST = (l: string) => `ol-last-engraving-${l}`;
  let lastChosen = $state<string | null>(null);
  $effect(() => { try { lastChosen = localStorage.getItem(LAST(laser)); } catch { lastChosen = null; } });
  const preferred = $derived(recipes.find((r) => r.id === lastChosen && (!draft.recipe || sameMaterial(r.name, draft.recipe.name))) ?? recipes[0] ?? null);
  /** The recipe this layer engraves with: its own, else the job's engraving material, else the one it would get. */
  const engravingRecipe = $derived.by(() => {
    const own = draft.layers.find((l) => l.name === NAME)?.recipe;
    const id = own && isEngraving(own) ? own.id : jobEngraves ? draft.recipe!.id : preferred?.id;
    return (server.doc?.library.recipes ?? []).find((r) => r.id === id) ?? null;
  });
  const fixedHeight = $derived(engravingRecipe?.attributes['OpenLaserEngravingHeight'] === 'fixed');
  /** Fixed or regulated height, saved in the engraving recipe itself, so
   *  every job engraving with it does the same. */
  async function setHeight(fixed: boolean): Promise<void> {
    const recipe = engravingRecipe;
    if (!recipe || fixed === fixedHeight) return;
    const key = 'OpenLaserEngravingHeight';
    try {
      await api.updateRecipe(recipe.id, { attributes: { [key]: fixed ? 'fixed' : 'follow' }, expected_attributes: { [key]: recipe.attributes[key] ?? null } });
      ui.say(fixed ? tr('Engraving height: fixed') : tr('Engraving height: regulated'));
    } catch (error) { ui.say(explain(error), true); }
  }
  function remember(recipe: RecipeView): void { lastChosen = recipe.id; try { localStorage.setItem(LAST(recipe.laser), recipe.id); } catch { /* not kept */ } }
  const fills = $derived(featureEdits.value(draft).layers ?? []);
  const fillOfLayer = (name: string): Fill | null => fills.find((l) => l.name === name)?.machining?.fill ?? null;
  let busy = $state(false);
  let previewBusy = $state(false);
  let pendingFill = $state<Fill | null>(null);
  const areaFill = $derived(pendingFill ?? fillOfLayer(NAME) ?? { ...defaultFill(), outline: false });
  const staged = $derived(engravingFeatures(draft, selectedContours, regions, pickMode === 'area' ? areaFill : null, NAME));
  const shownError = $derived(ui.engravingPreview?.error ?? draft.error);
  const refusedPreview = $derived(!!ui.engravingPreview?.error && ui.engravingPreview.error !== draft.error);
  let sequence = 0;
  $effect(() => {
    const features = structuredClone($state.snapshot(staged)) as Features;
    const revision = draft.revision;
    const ticket = ++sequence;
    previewBusy = true;
    const timer = setTimeout(() => {
      api.previewFeatures(features, revision).then(reply => {
        if (ticket === sequence && server.doc?.draft?.revision === revision) {
          ui.engravingPreview = { preview: reply.preview, features, error: reply.error, revision };
        }
      }).catch(error => { if (ticket === sequence) ui.say(explain(error), true); })
        .finally(() => { if (ticket === sequence) previewBusy = false; });
    }, 80);
    return () => { clearTimeout(timer); sequence++; };
  });
  $effect(() => () => { ui.engravingPreview = null; });

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
  /** The job's own material is an engraving recipe: new engraving layers follow it. */
  const jobEngraves = $derived(!!draft.recipe && isEngraving(draft.recipe));
  /** A layer engraves: marked, filled, with the preferred engraving recipe when it has none
   *  and the job's material does not engrave itself. */
  async function engrave(layer: string, recipe: RecipeView | null = preferred): Promise<void> {
    const edits: LayerChange[] = [{ kind: 'mode', layer, mode: 'mark' }, { kind: 'output', layer, on: true }];
    const current = draft.layers.find((l) => l.name === layer);
    if (recipe && !current?.recipe && !jobEngraves) edits.push({ kind: 'recipe', layer, recipe: recipe.id });
    await change(...edits);
    if (current?.mode !== 'mark' && !fillOfLayer(layer)) writeFill(layer, recipe ? fillOf(recipe.attributes) : defaultFill());
  }
  /** The selected shapes onto the engraving layer, which then engraves. */
  async function engraveSelection(): Promise<void> {
    const snapshot = ui.engravingPreview;
    if (previewBusy || refusedPreview || !snapshot || snapshot.revision !== draft.revision) return;
    busy = true;
    try {
      await api.setFeatures(snapshot.features, snapshot.revision);
      if (selectedContours.length && !draft.layers.find(l => l.name === NAME)?.recipe && preferred && !jobEngraves) {
        await api.changeLayers({ kind: 'recipe', layer: NAME, recipe: preferred.id });
      }
      // The panel stays open: the layer's recipe and lines are set right here.
      ui.say(tr("Engraving selection saved. Choose the recipe below; Undo restores the previous engraving."));
    } catch (error) { ui.say(explain(error), true); }
    finally { busy = false; }
  }
  async function useRecipe(layer: DraftLayer, recipe: RecipeView): Promise<void> {
    try {
      await change({ kind: 'recipe', layer: layer.name, recipe: recipe.id });
      if (isEngraving(recipe)) remember(recipe);
      const currentFill = fillOfLayer(layer.name);
      if (currentFill) writeFill(layer.name, { ...fillOf(recipe.attributes), outline: currentFill.outline, outline_holes: currentFill.outline_holes });
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
  const layerOf = $derived(new Map([...(draft.preview?.contours ?? []), ...(draft.preview?.engraving_regions ?? [])].flatMap(c => c.sources.map(source => [source,c.layer] as const))));
  /** Selected shapes on an engraving layer. */
  const engravedSelection = $derived(selectedContours.filter((i) => marked.some((l) => l.name === layerOf.get(i))));
  async function cutOn(layer: string): Promise<void> {
    const contours = [...engravedSelection];
    try {
      await change({ kind: 'assign', contours, layer });
      ui.say(tr("{0} shapes are cut on {1} again · Undo takes it back", contours.length, layer));
    } catch { /* said */ }
  }
  let focused = $state<number | null>(null);
  function editGeometry(source: number) {
    const placed=draft.placed[source]; if(!placed) return;
    const part=draft.parts.find(p=>placed.source>=p.first && placed.source<p.first+p.contours);
    if(part) void editPart(part.id,placed.source-part.first);
  }
  const close = () => (ui.setupPanel = null);
  /** Picks for another engraving layer: its shapes come back selected. */
  function workOn(name: string): void {
    ui.engravingLayer = name; ui.engravingSession++; ui.engravingPreview = null;
  }
  /** A fresh engraving layer, with its own recipe to come. */
  function newLayer(): void {
    const names = new Set(draft.layers.map((l) => l.name));
    let number = 1; while (names.has('Gravur ' + number)) number++;
    workOn('Gravur ' + number);
  }
</script>

<div class="feat-head">
  <h2 class="feat-title"><i class="ic ic-tool-engrave"></i>{NAME}</h2>
  <button class="btn btn-ghost" onclick={close}>{tr("Close")}</button>
</div>
<p class="feat-desc">{tr("Engraving marks the surface line by line. Pick the shapes to engrave on the drawing, then set the lines here.")}</p>

<div class="stack">
  <div class="recipes" role="group" aria-label={tr("Select engraving geometry")}>
    <button class="chip" class:on={pickMode === 'area'} onclick={() => (pickMode = 'area')}>{tr("Select areas")}</button>
    <button class="chip" class:on={pickMode === 'contour'} onclick={() => (pickMode = 'contour')}>{tr("Select contours")}</button>
  </div>
  <p class="muted">{tr("Area: click the face to fill. A 0 ring keeps its hole clear; click the hole to fill only that inner face. Contour: trace only the clicked outline.")}</p>
  <p class="muted">{tr("Click adds to the engraving preview; click again removes it. Apply saves the selection; Close cancels unsaved selection changes.")}</p>
  {#if previewBusy || busy || draft.error === 'preparing geometry'}<p role="status">{tr("Updating engraving preview…")}</p>{/if}
  <p class="muted">{tr("Selected now: {0} contours", selectedContours.length)}</p>
  <h3>{tr("Contours in engraving preview")}</h3>
  <ol class="contour-list">
    {#each selectedContours as source}<li class:focused={focused === source}>
      <button class="btn btn-ghost" onclick={() => { focused=source; onfocus(source); }}>{tr("Show contour {0}",source+1)}</button>
      <div class="row"><button class="btn btn-ghost" onclick={() => editGeometry(source)}>{tr("Edit geometry")}</button><button class="btn btn-ghost" onclick={() => onremove(source)}>{tr("Remove from engraving")}</button></div>
    </li>{:else}<li class="muted">{tr("No contours selected for engraving.")}</li>{/each}
  </ol>
  <button class="btn btn-ghost block" disabled={!selectedContours.length} onclick={onclear}>{tr("Clear selection")}</button>
  <button class="btn btn-primary block" disabled={busy || previewBusy || refusedPreview || !ui.engravingPreview || (!selectedContours.length && !draft.layers.some(l => l.name === NAME && l.mode === 'mark'))} onclick={engraveSelection}>
    {tr("Done")} · {NAME}
  </button>
  <button class="btn btn-ghost block" disabled={busy || previewBusy} onclick={newLayer}><i class="ic ic-plus"></i>{tr("New engraving layer")}</button>
  {#if engravingRecipe && engravingRecipe.laser === 'fiber'}
    <h3>{tr("Engraving height")}</h3>
    <div class="seg block" role="group" aria-label={tr("Engraving height mode")}>
      <button class:on={!fixedHeight} aria-pressed={!fixedHeight} onclick={() => setHeight(false)}>{tr("Regulated height")}</button>
      <button class:on={fixedHeight} aria-pressed={fixedHeight} onclick={() => setHeight(true)}>{tr("Fixed height")}</button>
    </div>
    <p class="muted">{tr("Saved in the recipe {0}", engravingRecipe.name)}</p>
  {/if}
  {#if pickMode === 'area' && !pictures.has(NAME)}<FillSettings fill={areaFill} apply={c => (pendingFill = { ...areaFill, ...c })} speed={speedOf(draft.layers.find(l => l.name === NAME) ?? draft.layers[0]!)} />{/if}
  <h3>{tr("Layer visibility")}</h3>
  <div class="stack">
    {#each [...new Set([...draft.layers.map(l => l.name), ...(selectedContours.length ? [NAME] : [])])] as name}
      <button class="btn btn-ghost block" aria-pressed={!ui.drawingLayerHidden(name)} onclick={() => ui.toggleDrawingLayer(name)}>
        <i class="ic {ui.drawingLayerHidden(name) ? 'ic-eye-off' : 'ic-eye'}"></i>{name} · {tr(ui.drawingLayerHidden(name) ? tr("Show layer") : tr("Hide layer"))}
      </button>
    {/each}
  </div>
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

  {#if shownError && shownError !== 'preparing geometry'}
    <p class="warn-box" role="alert">{tr("Job geometry error: {0}", shownError)}<br />{tr("The complete job could not be prepared. Valid engraving layers are previewed separately; fix the geometry error before running.")}</p>
  {/if}
  {#if !draft.recipe}
    <p class="warn-box">{tr("No job recipe selected. Layers with their own recipe can still engrave; choose a recipe for the other layers before running.")}</p>
  {/if}
  {#each marked as layer (layer.name)}
    {@const fill = fillOfLayer(layer.name)}
    {@const closed = closedOn(layer.name)}
    {@const hidden = ui.drawingLayerHidden(layer.name)}
    {@const used = layer.recipe ?? draft.recipe}
    <section class="layer" class:hidden>
      <header>
        <span class="layer-swatch mark" style:--layer={swatchColor(draft.layers, layer.name)}></span>
        <strong>{layer.name}</strong>
        <small>{tr("On this layer: {0} contours", layer.contours)}</small>
        {#if layer.name === NAME}<small class="editing">{tr("Selecting")}</small>{:else}<button class="btn btn-ghost" disabled={busy || previewBusy} onclick={() => workOn(layer.name)}>{tr("Select shapes for this layer")}</button>{/if}
        <button class="icon-btn" aria-label={hidden ? tr("Show on the drawing") : tr("Hide on the drawing")} onclick={() => ui.toggleDrawingLayer(layer.name)}>
          <i class="ic {hidden ? 'ic-eye-off' : 'ic-eye'}"></i>
        </button>
      </header>
      {#if !layer.chosen}
        <p class="warn-box">{tr("This layer has no recipe yet, so nothing on it is worked out or engraved. Choose one:")}</p>
      {/if}
      <details class="recipe-picker">
        <summary>{tr("Recipe")}: {used ? `${materialLabel(used.name)} · ${used.attributes['CutSpeed'] ?? '?'} mm/s` : tr("Choose one")}{#if used && !layer.recipe} · {tr("from the material")}{:else if layer.recipe} · {tr("own recipe")}{/if}</summary>
      <div class="recipes" role="group" aria-label={tr("Engraving recipes")}>
        {#if draft.recipe}
          <button class="chip" class:on={!layer.recipe} disabled={busy} onclick={() => change({ kind: 'recipe', layer: layer.name, recipe: null }).catch(() => undefined)}>{tr("Material")}: {materialLabel(draft.recipe.name)} · {draft.recipe.attributes['CutSpeed'] ?? '?'} mm/s</button>
        {/if}
        {#each recipes as r (r.id)}
          <button class="chip" class:on={layer.recipe?.id === r.id} disabled={busy} onclick={() => useRecipe(layer, r)}>{materialLabel(r.name)} · {r.attributes['CutSpeed'] ?? '?'} mm/s · {tr(r.gas)}</button>
        {:else}
          <p class="muted">{tr("No engraving recipe yet: in Materials, open a recipe and choose Make it an engraving recipe.")}</p>
        {/each}
        <button class="link" onclick={() => (ui.layerSheet = layer.name)}>{tr("All layer settings")}</button>
      </div>
      </details>
      {#if !pictures.has(layer.name)}
        <div class="seg block" role="group" aria-label={tr("How this layer engraves")}>
          <button class:on={!fill} aria-pressed={!fill} disabled={busy} onclick={() => { if (fill) writeFill(layer.name, null); }}>{tr("Line only")}</button>
          <button class:on={!!fill} aria-pressed={!!fill} disabled={busy} onclick={() => { if (!fill) writeFill(layer.name, used ? fillOf(used.attributes) : defaultFill()); }}>{tr("Fill area")}</button>
        </div>
      {/if}
      {#if fill && draft.recipe && !draft.error && closed === 0}
        <p class="warn-box">{tr("No closed shape on this layer: a fill engraves inside closed outlines only. Close the outlines in the sketch, or the lines are traced as they are.")}</p>
      {/if}
      {#if fill && (layer.name !== NAME || pictures.has(layer.name))}
        <FillSettings {fill} apply={(c) => writeFill(layer.name, { ...fill, ...c })} speed={speedOf(layer)} offset={offsetOf(layer, fill)} image={pictures.has(layer.name)} />

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
  .contour-list { list-style:none; padding:0; margin:0; display:grid; gap:8px; max-height:300px; overflow:auto; }
  .contour-list li { border:1px solid var(--line); border-radius:8px; padding:6px; }
  .contour-list .focused { border-color:var(--accent); }
  .contour-list .row { flex-wrap:wrap; }
  .contour-list .btn { height:auto; min-height:44px; white-space:normal; }
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
