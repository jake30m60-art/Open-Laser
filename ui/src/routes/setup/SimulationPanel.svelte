<script lang="ts">
  import { t } from '../../i18n/index';
  import { excludedSources } from '../../lib/geometry-issues';
  import { server } from '../../stores/server.svelte';
  import { ui } from '../../stores/ui.svelte';
  import { simulationSegments, orderPosition, simulationDistanceProgress, simulationTimeProgress, simulationSteps } from '../../lib/cut-order';
  let { progress = $bindable(0), mode = $bindable('all') }: { progress?: number; mode?: 'all' | 'cut' | 'mark' } = $props();
  let playing = $state(false), speed = $state(1);
  const draft = $derived(server.doc!.draft!);
  const segments = $derived(simulationSegments(draft.preview, new Set(draft.layers.filter(l => l.mode === 'mark').map(l => l.name)), mode, excludedSources(draft.error,draft.features.only_contours,draft.preview?.contours.flatMap(c=>c.sources) ?? [])));
  const layerOf = (i:number) => draft.preview?.contours[i]?.layer ?? '';
  const position = $derived(orderPosition(segments,simulationDistanceProgress(segments,progress,layerOf)));
  const steps = $derived(simulationSteps(draft.preview,segments));
  const marks = $derived(new Set(draft.layers.filter(l=>l.mode==='mark').map(l=>l.name)));
  const length = $derived(segments.reduce((sum,s) => sum + s.length, 0));
  $effect(() => { void draft.revision; void mode; playing = false; progress = 0; });
  $effect(() => {
    if (!playing) return;
    let last = performance.now(), frame = 0;
    const tick = (now: number) => {
      progress = Math.min(1, progress + (now - last) * speed / 20_000); last = now;
      if (progress >= 1) playing = false; else frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  });
  function step(direction: number) {
    playing = false;
    const offsets: number[] = [0]; let walked = 0, previous = -1;
    for (const segment of segments) { if (segment.contour !== previous) offsets.push(simulationTimeProgress(segments,length ? walked / length : 0,layerOf)); walked += segment.length; previous = segment.contour; }
    offsets.push(1);
    progress = direction > 0 ? offsets.find(p => p > progress + 0.00001) ?? 1 : offsets.filter(p => p < progress - 0.00001).at(-1) ?? 0;
  }
</script>
<div class="feat-head"><h2 class="feat-title">{t("Simulation")}</h2><button class="btn btn-ghost" onclick={() => (ui.setupPanel = null)}>{t("Close")}</button></div>
<p class="muted">{t("Geometry preview only; playback speed is not machine time.")} {t("Each layer gets equal playback time, so short cuts remain visible.")}</p>
{#if draft.error}<p class="warn-box" role="alert">{t("Partial simulation: {0}. Fix this before running the job.", draft.error)}</p>{/if}
<p class="simulation-legend"><span style="color:#2563eb">● Schnitt bearbeitet</span> · <span style="color:#f97316">● Gravur bearbeitet</span> · <span style="color:#64748b">┄ Fahrt ohne Laser</span></p>
<div class="stack">
  {#if draft.features.only_contours}<p class="warn-box">{t("Output limited to {0} selected contours. Work all restores the complete job.",draft.features.only_contours.length)}</p>{/if}
  <div class="row" role="group" aria-label={t("Simulated operations")}>
    {#each ['all', 'cut', 'mark'] as choice}<button class="chip" class:on={mode === choice} onclick={() => (mode = choice as typeof mode)}>{t(choice === 'all' ? "Cut and engraving" : choice === 'cut' ? "Cut only" : "Engraving only")}</button>{/each}
  </div>
  <p>{Math.round(progress * 100)} % · {t("Path {0} / {1}", position ? position.contour + 1 : 0, draft.preview?.contours.length ?? 0)}</p>
  <input aria-label={t("Simulation position")} type="range" min="0" max="1" step="0.001" bind:value={progress} disabled={!segments.length} oninput={() => (playing = false)} />
  <button class="btn btn-primary" disabled={!segments.length} onclick={() => { if (progress >= 1) progress = 0; playing = !playing; }}>{t(playing ? "Pause preview" : "Play simulation")}</button>
  <div class="row"><button class="btn btn-ghost" onclick={() => step(-1)}>{t("Previous path")}</button><button class="btn btn-ghost" onclick={() => step(1)}>{t("Next path")}</button></div>
  <button class="btn btn-ghost" onclick={() => { playing = false; progress = 0; }}>{t("Restart simulation")}</button>
  <label>{t("Playback speed")}<select bind:value={speed}>{#each [0.25, 0.5, 1, 2, 4] as value}<option value={value}>{value}×</option>{/each}</select></label>
  <button class="btn btn-ghost" aria-pressed={ui.layerShown('travel')} onclick={() => ui.toggleLayer('travel')}>{t(ui.layerShown('travel') ? "Hide travel moves" : "Show travel moves")}</button>
  <h3>{t("Machining sequence")}</h3>
  <ol class="sequence">
    {#each steps as item, index}<li><button class="btn btn-ghost" class:current={!!position && item.contours.includes(position.contour)} aria-current={position && item.contours.includes(position.contour) ? 'step' : undefined} onclick={() => { playing = false; progress = Math.min(1,simulationTimeProgress(segments,length ? item.distance/length : 0,layerOf)+1e-9); }}>
      <strong>{index+1}. {t(marks.has(item.layer) ? "Engraving" : "Cut")}</strong><span>{t("Layer")}: {item.layer}</span><small>{t("Contours")}: {item.sources.map(i=>i+1).join(', ')}</small>
    </button></li>{/each}
  </ol>
  <p class="muted">{t("Cut order remains in its own tab. Simulation changes no job settings.")}</p>
</div>
<style>input { width:100%; min-height:44px; } .row { flex-wrap:wrap; } select { margin-left:12px; min-height:44px; } .stack>.btn { min-height:44px; white-space:normal; } .sequence { list-style:none; padding:0; margin:0; display:grid; gap:6px; max-height:320px; overflow:auto; } .sequence .btn { width:100%; height:auto; min-height:52px; display:grid; text-align:left; gap:3px; white-space:normal; } .sequence .current { border:2px solid var(--accent); background:var(--panel-2); } .sequence small { color:var(--ink-3); }</style>
