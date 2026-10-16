<script lang="ts">
  import { t as tl } from '../../i18n/index';
  import { t } from '../../i18n/index';

  // One machining feature's settings: every field the server takes, in
  // the words the operator uses. Places on the drawing are picked on the
  // canvas; the panel only starts and ends the picking. Each feature's form
  // is its own component under features/.
  import { featureEdits } from '../../stores/feature-edits';
  import { server } from '../../stores/server.svelte';
  import { ui, type FeatureId } from '../../stores/ui.svelte';
  import { ago, explain, recipeLabel } from '../../lib/format';
  import { TOOLS, defaultBridges, defaultCommon, defaultCooling, defaultFly, defaultJoints, defaultKerf, defaultLeads, isOn, toggled } from '../../lib/features';
  import OrderTree from './OrderTree.svelte';
  import BridgesForm from './features/BridgesForm.svelte';
  import CommonForm from './features/CommonForm.svelte';
  import FlyForm from './features/FlyForm.svelte';
  import CoolingForm from './features/CoolingForm.svelte';
  import JointsForm from './features/JointsForm.svelte';
  import KerfForm from './features/KerfForm.svelte';
  import LeadsForm from './features/LeadsForm.svelte';
  import OrderForm from './features/OrderForm.svelte';
  import StartForm from './features/StartForm.svelte';
  import type { PickFeature, SetFeatures } from './features/form';
  import type { Features } from '../../api';
  import { hasOwn, LAYER_MACHINING, scoped, setOwn, swatchColor } from '../../lib/drawing-layers';
  import { SELECTION_MACHINING, clearSelection, selectionHasOwn, selectionView, setSelection } from '../../lib/selection-machining';
  import type { MachiningPart } from '../../api';

  let { id, exit = false, ontoggle, selectedContours, onfocus = () => {}, compact = false }: {
    id: FeatureId;
    /** Leads: the exit lead and seam instead of the entry lead. */
    exit?: boolean;
    ontoggle: () => void;
    selectedContours: number[];
    onfocus?: (source:number) => void;
    compact?: boolean;
  } = $props();
  const doc = $derived(server.doc!);
  const draft = $derived(doc.draft!);
  const tool = $derived(TOOLS.find((t) => t.id === id)!);
  const base = $derived(featureEdits.value(draft));
  /** Leads, joints, cooling, kerf and start can be a layer's own. */
  const perLayer = $derived((LAYER_MACHINING as readonly string[]).includes(id) && (draft.layers.length > 1 || draft.layers.some(l => l.mode === 'mark')));
  const layerScope = $derived(perLayer && draft.layers.some((l) => l.name === ui.machiningLayer) ? ui.machiningLayer : null);
  /** With contours selected, leads, joints, cooling, kerf and start are
   *  edited for those contours alone, unless the operator picks a wider scope. */
  const perSelection = $derived((SELECTION_MACHINING as readonly string[]).includes(id) && selectedContours.length > 0);
  let wider = $state(false);
  $effect(() => { void selectedContours.join(','); wider = false; });
  const inSelection = $derived(perSelection && !wider);
  const chosen = $derived(inSelection ? [...selectedContours] : []);
  /** The layer the first chosen contour is on, which it otherwise follows. */
  const chosenLayer = $derived(inSelection ? draft.preview?.contours.find((c) => c.sources.includes(chosen[0]!))?.layer ?? null : null);
  const scope = $derived(inSelection ? null : layerScope);
  const features = $derived(inSelection ? selectionView(base, chosen[0]!, chosenLayer) : scope ? scoped(base, scope) : base);
  const own = $derived(scope ? hasOwn(base, scope) : false);
  const selectionOwn = $derived(inSelection && selectionHasOwn(base, chosen, id as MachiningPart));
  const on = $derived(isOn(features, id));
  const order = $derived(draft.layers.map((l) => l.name));
  const tab = $derived(exit ? 1 : 0);

  /** An edit to the job's machining; with a layer chosen, to that layer's
   *  own; with contours selected, to theirs alone. */
  const set: SetFeatures = (change) => {
    const layer = scope;
    const contours = chosen, contourLayer = chosenLayer, part = id as MachiningPart;
    const edit = inSelection
      ? (f: Features) => { const view = structuredClone(selectionView(f, contours[0]!, contourLayer)); change(view); setSelection(f, contours, part, view); }
      : layer
        ? (f: Features) => { const view = structuredClone(scoped(f, layer)); change(view); setOwn(f, layer, view, order); }
        : change;
    // Places picked but not yet applied are kept: changing a setting while
    // picking takes the picks along instead of dropping them.
    const kept = pendingPicks();
    featureEdits.change((f) => { kept?.(f); edit(f); }).catch((error) => ui.say(explain(error), true));
  };
  /** Copies the open picking session's places into a change, and ends it. */
  function pendingPicks(): ((f: Features) => void) | null {
    const picking = ui.picking;
    if (!picking || !picking.marks.length || picking.revision !== draft.revision) return null;
    const picked = structuredClone($state.snapshot(picking.features)) as Features;
    const copy = picking.feature === 'start' ? (f: Features) => { f.start.spots = picked.start.spots; }
      : picking.feature === 'joints' ? (f: Features) => { if (f.joints && picked.joints) f.joints.placement = picked.joints.placement; }
      : picking.feature === 'cooling' ? (f: Features) => { if (f.cooling && picked.cooling) f.cooling.placement = picked.cooling.placement; }
      : null;
    if (copy) ui.picking = null;
    return copy;
  }
  /** The layer goes back to the job's machining. */
  const useJob = () => { const layer = scope; if (layer) featureEdits.change((f) => setOwn(f, layer, null, order)).catch((error) => ui.say(explain(error), true)); };
  /** The selected contours follow their layer or the job again for this feature. */
  const useLayerForSelection = () => {
    const contours = chosen, part = id as MachiningPart;
    featureEdits.change((f) => clearSelection(f, contours, part)).catch((error) => ui.say(explain(error), true));
  };
  /** On or off; for a layer or a selection, its own. */
  function toggle(): void {
    if (!scope && !inSelection) { ontoggle(); return; }
    set((f) => { Object.assign(f, toggled(f, id)); });
  }

  /** Starts placing on the drawing; the canvas takes it from here. Places
   *  picked on the drawing are the job's, whichever layer they are on. */
  function pick(feature: PickFeature): void {
    if (scope) { ui.say(t("Places picked on the drawing apply to every layer. Choose All layers to pick them.")); return; }
    // Picked places are the job's: a selection's view must not become the job's machining.
    const staged = structuredClone($state.snapshot(inSelection ? base : features));
    if (feature === 'bridges' && !staged.bridges) staged.bridges = defaultBridges();
    ui.picking = { feature, first: null, order: [], revision: draft.revision, features: staged, marks: [] };
  }

  function reset(): void {
    set((f) => {
      if (id === 'leads') f.leads = defaultLeads();
      if (id === 'joints') f.joints = defaultJoints();
      if (id === 'cooling') f.cooling = defaultCooling();
      if (id === 'kerf') f.kerf = defaultKerf();
      if (id === 'bridges') f.bridges = defaultBridges();
      if (id === 'common' && f.common) f.common = defaultCommon(f.common.contours);
      if (id === 'fly' && f.fly) f.fly = defaultFly(f.fly.contours);
      if (id === 'order') f.order = { strategy: 'as_drawn', inner_first: true, circles_first: false, spread_heat: false };
      if (id === 'start') { f.start = { position: 'automatic', direction: 'keep', spots: [] }; f.seam = 'seal'; }
    });
  }
  const close = () => (ui.setupPanel = null);
</script>

{#if !compact || tool.optional}
  <div class="feat-head">
    <h2 class="feat-title"><i class="ic ic-tool-{id}"></i>{compact ? t("Enabled") : exit ? t("Exit lead and seam") : tool.name}</h2>
    {#if tool.optional}<button class="switch" class:on={on} onclick={toggle} aria-label={t("On")}></button>{/if}
    {#if !compact}<button class="btn btn-ghost" onclick={close}>{t("Close")}</button>{/if}
  </div>
{/if}
<p class="feat-desc">{t(tool.desc)}</p>
{#if perSelection || perLayer}
  <div class="scope" role="group" aria-label={t("Applies to")}>
    {#if perSelection}
      <button class="chip" class:on={inSelection} onclick={() => (wider = false)}>{t('Selection ({0})', selectedContours.length)}{selectionHasOwn(base, selectedContours, id as MachiningPart) ? ' ·' : ''}</button>
    {/if}
    <button class="chip" class:on={!inSelection && !scope} onclick={() => { wider = true; ui.machiningLayer = null; }}>{perLayer ? t("All layers") : t("Whole job")}</button>
    {#each perLayer ? draft.layers : [] as layer (layer.name)}
      <button class="chip" class:on={scope === layer.name} onclick={() => { wider = true; ui.machiningLayer = layer.name; }}>
        <span class="layer-swatch" style:--layer={swatchColor(draft.layers, layer.name)}></span>{layer.name}{hasOwn(base, layer.name) ? ' ·' : ''}
      </button>
    {/each}
  </div>
  {#if inSelection}
    <p class="muted scope-note">
      {#if selectionOwn}{t('The selected contours have their own settings.')} <button class="link" onclick={useLayerForSelection}>{t('Use the layer’s or job’s again')}</button>
      {:else}{t('A change applies to the selected contours only.')}{/if}
      {t('Contours with their own settings have a dashed violet outline.')}
    </p>
  {:else if scope}
    <p class="muted scope-note">
      {#if own}{tl('{0} has its own settings.', scope)} <button class="link" onclick={useJob}>{tl("Use the job's")}</button>
      {:else if draft.layers.find((l) => l.name === scope)?.mode === 'mark'}{tl('{0} is a mark, so it has none; a change gives it its own.', scope)}
      {:else}{tl("{0} uses the job's settings; a change gives it its own.", scope)}{/if}
    </p>
  {:else if draft.layers.some((l) => hasOwn(base, l.name))}
    <p class="muted scope-note">{tl("Layers marked · keep their own settings.")}</p>
  {/if}
{/if}
{#if id === 'order'}<OrderTree preview={draft.preview} {features} {set} {onfocus} />{/if}
{#if featureEdits.error}
  <p class="muted">{featureEdits.error} <button class="link" onclick={() => featureEdits.discard()}>{t("Discard refused edits")}</button></p>
{/if}
{#if id === 'leads' && scope && base.layers?.find(l => l.name === scope)?.machining?.fill}
  <p class="muted">{t("Filled engraving uses scan run-ups at every hatch line. Entry and exit leads apply to contour paths.")}</p>
  <button class="btn btn-soft" onclick={() => (ui.setupPanel = 'engrave')}>{t("Edit engraving run-ups")}</button>
{/if}
{#if id === 'leads'}<p class="muted lead-hint">{t("Drag a lead to edit it. Selected copies update the matching lead.")}</p>{/if}
<div class="stack" style="opacity:{on ? 1 : 0.45}">
  {#if id === 'leads'}
    <LeadsForm {features} {on} {tab} selectedContours={inSelection ? [] : selectedContours} {set} {pick} preview={scope || inSelection ? null : draft.preview} whole={!scope && !inSelection} />
  {:else if id === 'joints'}
    <JointsForm {features} {on} tab={0} contours={draft.placed.length} {set} {pick} />
    <h3 class="more-head">{t("Behaviour")}</h3>
    <JointsForm {features} {on} tab={1} contours={draft.placed.length} {set} {pick} />
  {:else if id === 'cooling'}
    <CoolingForm {features} {on} contours={draft.placed.length} {set} {pick} />
  {:else if id === 'kerf'}
    <KerfForm {features} {on} {set} />
  {:else if id === 'bridges'}
    <BridgesForm {features} {on} {set} {pick} />
  {:else if id === 'common'}
    <CommonForm {features} {on} {selectedContours} error={draft.error} {set} />
  {:else if id === 'fly'}
    <FlyForm {features} {on} {selectedContours} error={draft.error} {set} />
  {:else if id === 'order'}
    <OrderForm {features} {on} preview={draft.preview} {set} {pick} />
  {:else if id === 'start'}
    <StartForm {features} {on} tab={0} {set} {pick} />
    <h3 class="more-head">{t("Seam")}</h3>
    <StartForm {features} {on} tab={1} {set} {pick} />
  {/if}
</div>
<div class="side-foot">
  <div class="source">
    {#if draft.feature_source}{tl('Prefilled from {0} ({1}) on {2}.', draft.feature_source.name, ago(draft.feature_source.at), recipeLabel(draft.recipe))}{:else}{tl('First job on {0}.', recipeLabel(draft.recipe))}{/if}
      {tl('Saved with this job; the next job on this recipe starts from these.')}
  </div>
  <div class="row">
    <button class="btn btn-ghost" onclick={reset}>{t("Reset to defaults")}</button>
    <button class="btn btn-primary" onclick={close}>{t("Done")}</button>
  </div>
</div>

<style>
  .lead-hint { font-size:var(--t-sm); }
  .more-head { margin:12px 0 0; padding-top:10px; border-top:1px solid var(--line); font-size:var(--t-md); }
  .feat-title { display:flex; align-items:center; gap:10px; }
  .feat-title .ic { width:24px; height:24px; color:var(--accent-2); }
  .scope { display:flex; flex-wrap:wrap; gap:6px; margin:0 0 8px; }
  .scope .chip { min-height:44px; display:inline-flex; align-items:center; gap:6px; }
  .layer-swatch { width:12px; height:12px; border-radius:3px; background:var(--layer); }
  .scope-note { font-size:var(--t-sm); margin:0 0 10px; }
</style>
