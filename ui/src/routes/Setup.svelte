<script lang="ts">
  import ManualCut from '../components/ManualCut.svelte';
  import JobOverview from './setup/JobOverview.svelte';
  import { t } from '../i18n/index';

  import SheetStrip from '../components/SheetStrip.svelte';
  import Canvas from './setup/Canvas.svelte';
  import JobPanel from './setup/JobPanel.svelte';
  import FeaturePanel from './setup/FeaturePanel.svelte';
  import LayerSheet from './setup/LayerSheet.svelte';
  import CopyPanel from './setup/CopyPanel.svelte';
  import ClipboardPanel from './setup/ClipboardPanel.svelte';
  import NestPanel from './setup/NestPanel.svelte';
  import EngravePanel from './setup/EngravePanel.svelte';
  import SimulationPanel from './setup/SimulationPanel.svelte';
  import CreateText from '../components/CreateText.svelte';
  import { featureEdits } from '../stores/feature-edits';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { explain } from '../lib/format';
  import { pasteable, type CopiedShapes, type PasteSettings } from '../lib/copy-paste';
  import { editableContours, fillAreas } from '../lib/viewer-geometry';
  import { TOOLS, isOn, stateOf, toggled } from '../lib/features';
  import ToolBar, { type Tool } from './setup/ToolBar.svelte';

  let { compact = false }: { compact?: boolean } = $props();
  let canvasPanel = $state<HTMLElement | null>(null);
  let settingsPanel = $state<HTMLElement | null>(null);
  const picking = $derived(!!ui.picking || ui.nestPicking);
  $effect(() => {
    if (compact && ui.setupPanel) settingsPanel?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  });
  $effect(() => {
    if (compact && picking) canvasPanel?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  });
  const doc = $derived(server.doc!);
  const draft = $derived(doc.draft);

  const EXTRA = [{ id: 'simulation', short: t("Simulation") }, { id: 'engrave', short: t("Engrave") }, { id: 'nest', short: t("Nest parts") }, { id: 'copy', short: t("Copy job") }];
  let texting = $state(false);
  let manualCutOpen=$state(false);
  let selectedContours = $state<number[]>([]);
  let engravingAreas = $state<Array<{ contour: number; holes: number[] }>>([]);
  let engraveMode = $state<'area' | 'contour'>('area');

  let simulationProgress = $state(0);
  let simulationMode = $state<'all' | 'cut' | 'mark'>('all');
  let canvas = $state<{ paste: (count?: number) => Promise<void>; selectAll: () => void; clearSelection: () => void; focusContour: (source:number) => void; removeEngraving: (source:number) => void; selectPart: (first: number, count: number) => void }>();
  let clipboard = $state<CopiedShapes | null>(null);
  let pasting = $state(false);
  let pasteSettings = $state<PasteSettings>({ count: 1, gap: 10, direction: 'right' });

  async function scopeOutput(only: boolean): Promise<void> {
    const ids = [...selectedContours];
    if (only && !ids.length) return;
    try {
      await featureEdits.change(features => {
        features.only_contours = only ? ids : undefined;
        if (only && draft?.preview) {
          const boundaries = editableContours(draft.preview.contours,draft.preview.engraving_regions);
          const filled = new Set(draft.layers.filter(l=>l.mode==='mark' && features.layers?.find(f=>f.name===l.name)?.machining?.fill).map(l=>l.name));
          const regions = fillAreas(boundaries,ids).filter(r=>boundaries.some(c=>c.sources[0]===r.contour && filled.has(c.layer)));
          features.fill_regions = [...new Map([...(features.fill_regions ?? []),...regions].map(r=>[r.contour,r])).values()];
        }
      });
    } catch (error) { ui.say(explain(error),true); }
  }

  function runTool(id: string): void {
    ui.setupPanel = ui.setupPanel === id ? null : (id as typeof ui.setupPanel);
    if (id === 'leads' && ui.setupPanel === 'leads') {
      const names = new Set((draft?.preview?.contours ?? []).filter(c => c.sources.some(s => selectedContours.includes(s))).map(c => c.layer));
      ui.machiningLayer = names.size === 1 ? [...names][0]! : null;
      for (const kind of ['lead_in', 'lead_out']) if (!ui.layerShown(kind)) ui.toggleLayer(kind);
    }
  }

  async function toggle(id: string): Promise<void> {
    if (!draft) return;
    if (id === 'common' && !draft.features.common && selectedContours.length < 2) {
      ui.say('Select at least two adjacent contours on the drawing.', true);
      return;
    }
    if (id === 'fly' && !draft.features.fly && selectedContours.length < 1) {
      ui.say('Select the contours to cut flying on the drawing, such as a grid of square holes.', true);
      return;
    }
    try {
      await featureEdits.change((features) => Object.assign(features, toggled(features, id, selectedContours)));
    } catch (error) {
      ui.say(explain(error), true);
    }
  }

  /** The machining bar's tools, laid out and ordered by the same ToolBar as the drawing bar. */
  const BAR_TOOLS = $derived(Object.fromEntries([
    ...TOOLS.map((t): [string, Tool] => [t.id, {
      label: t.short, when: 'always', icon: `ic-tool-${t.id}`, detail: () => (draft ? stateOf(draft.features, t.id) : ''),
      on: () => ui.setupPanel === t.id, set: () => !!draft && isOn(draft.features, t.id), disabled: () => false, run: () => runTool(t.id),
    }]),
    ...EXTRA.map((t): [string, Tool] => [t.id, {
      label: t.short, when: 'always', icon: t.id === 'simulation' ? 'ic-tool-order' : t.id === 'nest' ? 'ic-tool-nest' : t.id === 'engrave' ? 'ic-tool-engrave' : 'ic-copy', on: () => ui.setupPanel === t.id, disabled: () => false, run: () => runTool(t.id),
    }]),
  ]));

</script>

<section class="panel main canvas-panel" class:corner={!compact} bind:this={canvasPanel}>
  <ToolBar class="setup-head" variant="names" label="Machining tools" tools={BAR_TOOLS} order={ui.bar} save={(order) => ui.setBar(order)}
    bind:editing={ui.editBar} corner={!compact} />

  {#if draft}<div class="selection-output">
    <button class="btn btn-ghost" disabled={featureEdits.busy || !selectedContours.length} onclick={() => scopeOutput(true)}>{t("Work selected contours only")}</button>
    <button class="btn btn-ghost" disabled={featureEdits.busy || !draft.features.only_contours} onclick={() => scopeOutput(false)}>{t("Work all contours")}</button>
    {#if draft.features.only_contours}<strong role="status">{t("Output limited to {0} selected contours. Work all restores the complete job.",draft.features.only_contours.length)}</strong>{/if}
  </div>{/if}
  <SheetStrip />
  <Canvas {engraveMode} bind:engravingAreas bind:this={canvas} bind:selectedContours bind:clipboard bind:pasting {pasteSettings} {simulationProgress} {simulationMode} />
</section>

<aside class="panel side" class:spread={ui.setupPanel === null} bind:this={settingsPanel}>
  {#if draft}<JobOverview onmanualcut={()=>manualCutOpen=true} onselectpart={(first,count)=>canvas?.selectPart(first,count)} />{/if}
  {#if !draft}
    <h2>No part open</h2>
    <p class="muted">Pick a part or a saved job on the Parts page, or start from text.</p>
    <div class="side-foot"><button class="btn btn-ghost lg block" onclick={() => (texting = true)}>{t("Add text")}</button><button class="btn btn-primary lg block" onclick={() => (ui.tab = 'parts')}>Go to Parts</button></div>
  {:else if ui.setupPanel === 'copy'}
    <CopyPanel />
  {:else if ui.setupPanel === 'clipboard'}
    <ClipboardPanel
      {clipboard} bind:settings={pasteSettings} {pasting}
      disabled={draft.error === 'preparing geometry' || !pasteable(clipboard, draft) || !!ui.picking || ui.nestShown || ui.nestPicking}
      onpaste={() => { void canvas?.paste(pasteSettings.count); }} />
  {:else if ui.setupPanel === 'engrave'}
    <EngravePanel {selectedContours} regions={engravingAreas} bind:pickMode={engraveMode} onclear={() => canvas?.clearSelection()} onfocus={source => canvas?.focusContour(source)} onremove={source => canvas?.removeEngraving(source)} />
  {:else if ui.setupPanel === 'simulation'}
    <SimulationPanel bind:progress={simulationProgress} bind:mode={simulationMode} />
  {:else if ui.setupPanel === 'nest'}
    <NestPanel {selectedContours} onselectall={() => canvas?.selectAll()} />
  {:else if ui.setupPanel}
    <FeaturePanel id={ui.setupPanel} {selectedContours} {compact} onfocus={source => canvas?.focusContour(source)} ontoggle={() => toggle(ui.setupPanel!)} />
  {:else}
    <JobPanel onselectpart={(first, count) => canvas?.selectPart(first, count)} />
  {/if}
</aside>
{#if manualCutOpen}<ManualCut onclose={()=>manualCutOpen=false} />{/if}
{#if texting}<CreateText onclose={() => (texting = false)} />{/if}
{#if ui.layerSheet && draft}{#key ui.layerSheet}<LayerSheet name={ui.layerSheet} onclose={() => (ui.layerSheet = null)} />{/key}{/if}



<style>
  /* The machining bar runs along the top and down the left; the drawing bar spans the foot. */
  .canvas-panel.corner {
    display: grid; grid-template-columns: auto minmax(0, 1fr); grid-template-rows: auto auto auto minmax(0, 1fr) auto;
    grid-template-areas: 'top top' 'side scope' 'side strip' 'side canvas' 'bottom bottom';
  }
  .selection-output { display:flex; flex-wrap:wrap; align-items:center; gap:8px; padding:8px; font-size:var(--t-sm); }
  .canvas-panel.corner > .selection-output { grid-area:scope; }
  .canvas-panel.corner > :global(.sheet-strip) { grid-area: strip; }
  .canvas-panel.corner > :global(.canvas-wrap) { grid-area: canvas; }
  .canvas-panel.corner > :global(.drawing-toolbar) { grid-area: bottom; }
</style>
