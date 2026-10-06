<script lang="ts">
  // One layer, as LightBurn's cut settings have it: its colour and name,
  // whether it runs and shows, whether it cuts through or marks the
  // surface, the recipe it runs under, and its machining. Every change
  // applies at once; Undo takes it back.
  import { untrack } from 'svelte';
  import Modal from '../../components/Modal.svelte';
  import { api } from '../../api/client';
  import { server } from '../../stores/server.svelte';
  import { ui } from '../../stores/ui.svelte';
  import { osk } from '../../lib/osk.svelte';
  import { explain, plural, recipeLabel } from '../../lib/format';
  import { quantity } from '../../lib/units.svelte';
  import { materialsOf } from '../../lib/materials';
  import { defaultFill, hasOwn, PALETTE, setFill, swatchColor } from '../../lib/drawing-layers';
  import Stepper from '../../components/params/Stepper.svelte';
  import Toggle from '../../components/params/Toggle.svelte';
  import { featureEdits } from '../../stores/feature-edits';
  import type { Fill, LayerChange, LayerMode } from '../../api';

  let { name: initial, onclose }: { name: string; onclose: () => void } = $props();
  // Renaming changes the layer's name, so the sheet follows it.
  let name = $state(untrack(() => initial));
  const draft = $derived(server.doc!.draft!);
  const layer = $derived(draft.layers.find((l) => l.name === name) ?? null);
  // Gone, as Undo can make it; a rename in flight has it briefly missing.
  $effect(() => { if (!layer && !busy) onclose(); });
  const own = $derived(hasOwn(featureEdits.value(draft), name));
  // The job's laser's recipes, by material; the job's material first.
  const materials = $derived.by(() => {
    const job = draft.recipe;
    const all = (server.doc?.library.recipes ?? []).filter((r) => !job || r.laser === job.laser);
    return materialsOf(all).sort((a, b) => Number(b.name === job?.name) - Number(a.name === job?.name) || a.name.localeCompare(b.name));
  });
  let coloring = $state(false);
  let busy = $state(false);

  async function change(...edits: LayerChange[]): Promise<void> {
    busy = true;
    try {
      for (const edit of edits) {
        await api.changeLayers(edit);
        if (edit.kind === 'rename') name = edit.to;
      }
    } catch (error) { ui.say(explain(error), true); } finally { busy = false; }
  }
  function rename(): void {
    osk.text('Layer name', name, (to) => {
      const next = to.trim();
      if (!next || next === name) return;
      void change({ kind: 'rename', from: name, to: next });
    });
  }
  function color(value: string | null): void {
    coloring = false;
    const rgb = value ? ([1, 3, 5].map((i) => parseInt(value.slice(i, i + 2), 16)) as [number, number, number]) : null;
    void change({ kind: 'color', layer: name, color: rgb });
  }
  const setMode = (mode: LayerMode) => { if (layer && layer.mode !== mode) void change({ kind: 'mode', layer: name, mode }); };
  /** A recipe, or the material when `null`; choosing one also turns the layer on. */
  function use(recipe: string | null): void {
    const edits: LayerChange[] = [];
    if (!layer?.output) edits.push({ kind: 'output', layer: name, on: true });
    edits.push({ kind: 'recipe', layer: name, recipe });
    void change(...edits);
  }
  const hidden = $derived(ui.drawingLayerHidden(name));
  function machining(): void {
    ui.machiningLayer = name;
    ui.setupPanel = 'leads';
    onclose();
  }
  // A mark's scan fill: engraving its closed shapes line by line.
  const fill = $derived(featureEdits.value(draft).layers?.find((l) => l.name === name)?.machining?.fill ?? null);
  function writeFill(next: Fill | null): void {
    const order = draft.layers.map((l) => l.name);
    featureEdits.change((f) => setFill(f, name, next, order)).catch((error) => ui.say(explain(error), true));
  }
  const fillWith = (change: Partial<Fill>) => { if (fill) writeFill({ ...fill, ...change }); };
  const usesJob = $derived(!!layer && layer.output && layer.chosen && !layer.recipe);
  const swatch = $derived(swatchColor(draft.layers, name));
</script>

{#if layer}
<Modal title="Layer" {onclose}>
  <div class="layer-sheet">
    <div class="ident">
      <button class="swatch-btn" aria-label="Colour" disabled={busy} onclick={() => (coloring = !coloring)}>
        <span class="swatch" class:mark={layer.mode === 'mark'} style:--layer={swatch}></span>
      </button>
      <button class="name" disabled={busy} onclick={rename}>
        <strong>{layer.name}</strong><small>{plural(layer.contours, 'shape')} · Rename</small>
      </button>
      <button class="icon-btn" class:off={hidden} aria-label={hidden ? 'Show on the drawing' : 'Hide on the drawing'} onclick={() => ui.toggleDrawingLayer(name)}>
        <i class="ic {hidden ? 'ic-eye-off' : 'ic-eye'}"></i>
      </button>
    </div>
    {#if coloring}
      <div class="colors">
        {#each PALETTE as value (value)}
          <button class="color" style:--layer={value} aria-label="Colour {value}" onclick={() => color(value)}></button>
        {/each}
        <button class="btn btn-ghost from-file" onclick={() => color(null)}>From the file</button>
      </div>
    {/if}

    <div class="setting">
      <span><strong>Output</strong><small>{layer.output ? 'Runs with the job' : 'Skipped'}</small></span>
      <button class="switch" class:on={layer.output} disabled={busy} aria-label="Output"
        onclick={() => change({ kind: 'output', layer: name, on: !layer.output })}></button>
    </div>

    {#if layer.output}
      <div class="setting">
        <span><strong>Mode</strong><small>{layer.mode === 'cut' ? 'Through the sheet, with the machining' : 'Traces the surface, bare'}</small></span>
        <div class="seg" role="group" aria-label="Mode">
          <button class:on={layer.mode === 'cut'} disabled={busy} onclick={() => setMode('cut')}>Cut</button>
          <button class:on={layer.mode === 'mark'} disabled={busy} onclick={() => setMode('mark')}>Mark</button>
        </div>
      </div>

      {#if layer.mode === 'mark'}
        <div class="setting">
          <span><strong>Fill</strong><small>{fill ? 'Engraves the closed shapes line by line' : 'Traces the outlines only'}</small></span>
          <button class="switch" class:on={!!fill} disabled={busy} aria-label="Fill"
            onclick={() => writeFill(fill ? null : defaultFill())}></button>
        </div>
        {#if fill}
          <div class="fill">
            <Stepper label="Line spacing" value={fill.spacing} unit="mm" step={0.01} apply={(v) => fillWith({ spacing: Math.min(20, Math.max(0.01, v)) })} />
            <Stepper label="Angle" value={fill.angle} unit="°" step={15} apply={(v) => fillWith({ angle: v % 180 })} />
            <Stepper label="Overscan, laser off" value={fill.run_up} unit="mm" step={0.5} apply={(v) => fillWith({ run_up: Math.min(50, v) })} />
            <p class="fill-hint">How far every line runs past the engraving, so the head turns round off the work and engraves at full speed.</p>
            <Toggle label="Both directions" value={fill.bidirectional} apply={(v) => fillWith({ bidirectional: v })} />
            <p class="fill-hint">Off engraves every line under the same conditions: one axis, one direction, one speed, whatever the shape's strokes. The head returns with the laser off.</p>
            <Stepper label="Scan offset" value={fill.scan_offset} unit="mm" step={0.01} apply={(v) => fillWith({ scan_offset: Math.min(2, v) })} />
            <p class="fill-hint">Backlash compensation for both directions: half the shift you measure between lines engraved left and right.</p>
            <h4 class="fill-head">Passes</h4>
            <Stepper label="Passes" value={fill.passes} unit="" step={1} apply={(v) => fillWith({ passes: Math.min(20, Math.max(1, Math.round(v))) })} />
            <Stepper label="Turn each pass" value={fill.pass_angle} unit="°" step={15} apply={(v) => fillWith({ pass_angle: v % 360 })} />
            <Stepper label="Shift each pass" value={Math.round(fill.pass_shift * 100)} unit="%" step={25} apply={(v) => fillWith({ pass_shift: Math.min(99, v) / 100 })} />
            <p class="fill-hint">2 passes, 90° crosshatch. 2 passes, 0°, 50 % lays the second pass between the first one's lines: finer, without a slower line. 3 passes, 0°, 0 % engraves deeper.</p>
            <div class="presets">
              <button class="chip" onclick={() => fillWith({ passes: 1, pass_angle: 0, pass_shift: 0 })}>Single</button>
              <button class="chip" onclick={() => fillWith({ passes: 2, pass_angle: 90, pass_shift: 0 })}>Crosshatch</button>
              <button class="chip" onclick={() => fillWith({ passes: 2, pass_angle: 0, pass_shift: 0.5 })}>Interleaved</button>
              <button class="chip" onclick={() => fillWith({ passes: 3, pass_angle: 0, pass_shift: 0 })}>Deeper ×3</button>
            </div>
            <Toggle label="Trace the outlines after the fill" value={fill.outline} apply={(v) => fillWith({ outline: v })} />
          </div>
        {/if}
      {/if}

      <div class="setting">
        <span><strong>Machining</strong><small>{own ? 'Its own' : layer.mode === 'mark' ? 'None' : 'The job\'s'}</small></span>
        <button class="btn btn-ghost" onclick={machining}>Edit</button>
      </div>

      <h3 class:warn-text={!layer.chosen}>Recipe{!layer.chosen ? ' · choose one' : ''}</h3>
      <p class="recipe-note">{layer.mode === 'cut' ? 'Cuts run on the material unless you choose another.' : 'The material\'s recipe would cut through: choose an engraving or marking recipe.'}</p>
      <div class="recipes">
        <button class="choice" class:on={usesJob} disabled={busy} onclick={() => use(null)}>
          <strong>Material</strong><small>{draft.recipe ? recipeLabel(draft.recipe) : 'Not chosen yet'}</small>
        </button>
        {#each materials as m (m.key)}
          <div class="material">
            <span class="mat-name">{m.name}</span>
            <div class="chips">
              {#each m.recipes as r (r.id)}
                <button class="chip" class:on={layer.recipe?.id === r.id} disabled={busy} onclick={() => use(r.id)}>
                  {r.thickness_mm > 0 ? quantity(r.thickness_mm, 'mm') : '—'} · {r.gas}
                </button>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</Modal>
{/if}

<style>
  .layer-sheet { display:grid; gap:10px; }
  .fill { display:grid; gap:6px; padding:0 0 6px; }
  .fill-hint { margin:-2px 0 4px; font-size:var(--t-sm); color:var(--ink-3); }
  .fill-head { margin:8px 0 0; font-size:var(--t-sm); color:var(--ink-2); }
  .presets { display:flex; flex-wrap:wrap; gap:6px; }
  .presets .chip { min-height:44px; }
  .ident { display:flex; align-items:center; gap:6px; min-width:0; }
  .swatch-btn, .icon-btn { flex:none; width:48px; height:48px; display:grid; place-items:center; border:1px solid var(--line); border-radius:12px; background:var(--panel-2); color:var(--ink-2); cursor:pointer; }
  .icon-btn.off { color:var(--ink-3); }
  .swatch { width:24px; height:24px; border-radius:6px; background:var(--layer); }
  .swatch.mark { background:transparent; border:3px dashed var(--layer); }
  .name { flex:1; min-width:0; min-height:48px; display:grid; gap:2px; padding:0 8px; border:0; background:transparent; color:var(--ink); text-align:left; cursor:pointer; }
  .name strong { font-size:var(--t-lg); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .name small, .setting small, .choice small { font-size:var(--t-sm); color:var(--ink-3); }
  .colors { display:grid; grid-template-columns:repeat(5, 1fr); gap:8px; }
  .color { height:44px; border:2px solid var(--line); border-radius:10px; background:var(--layer); cursor:pointer; }
  .from-file { grid-column:1 / -1; min-height:44px; }
  .setting { display:flex; align-items:center; justify-content:space-between; gap:12px; min-height:52px; padding:4px 0; border-top:1px solid var(--line); }
  .setting > span { display:grid; gap:2px; min-width:0; }
  .setting strong { font-size:var(--t-base); }
  .setting .seg button { min-height:44px; padding:0 16px; }
  .setting .btn { min-height:44px; }
  h3 { margin:6px 0 0; padding-top:12px; border-top:1px solid var(--line); }
  .recipe-note { margin:-4px 0 0; font-size:var(--t-sm); color:var(--ink-3); }
  .recipes { display:grid; gap:10px; max-height:42vh; overflow-y:auto; overscroll-behavior:contain; }
  .choice { min-height:52px; display:grid; gap:2px; padding:8px 14px; border:1px solid var(--line); border-radius:10px; background:var(--panel-2); color:var(--ink); font:inherit; text-align:left; cursor:pointer; }
  .choice.on { border-color:var(--accent); background:var(--accent-soft); }
  .material { display:grid; gap:6px; }
  .mat-name { font-weight:700; font-size:var(--t-sm); color:var(--ink-2); }
  .chips { display:flex; flex-wrap:wrap; gap:6px; }
  .chips .chip { min-height:44px; }
</style>
