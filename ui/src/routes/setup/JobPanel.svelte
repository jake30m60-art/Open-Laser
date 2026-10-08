<script lang="ts">
  import { t as tl } from '../../i18n/index';
  import { t } from '../../i18n/index';

  import { beginProject, editPart } from '../../lib/sketch-workflow';
  import { plain } from '../../lib/plain';
  import { displayNumber, quantity, unitLabel } from '../../lib/units.svelte';
  import MaterialSheet from './MaterialSheet.svelte';
  import MaterialSummary from '../../components/MaterialSummary.svelte';
  import PreflightEditor from '../../components/PreflightEditor.svelte';
  import { pictureOf } from '../../lib/materials';
  import { api } from '../../api/client';
  import { server } from '../../stores/server.svelte';
  import { ui } from '../../stores/ui.svelte';
  import { explain, laserLabel, plural, recipeLabel, seconds } from '../../lib/format';
  import { preflightLabel, saveJob, saveLabel } from '../../lib/save-job';
  import type { DraftLayer } from '../../api';
  import { TOOLS, isOn } from '../../lib/features';
  import AddParts from '../../components/AddParts.svelte';
  import CreateText from '../../components/CreateText.svelte';
  import GasLaserCard from '../../components/GasLaserCard.svelte';

  const doc = $derived(server.doc!);
  const draft = $derived(doc.draft!);
  const recipe = $derived(draft.recipe);
  /** Selects a part's shapes on the drawing, when there is one to select on. */
  let { onselectpart }: { onselectpart?: (first: number, count: number) => void } = $props();
  const nameOf = (id: string) => doc.library.parts.find((p) => p.id === id)?.name ?? t('Missing part');
  let adding = $state(false);
  let texting = $state(false);
  const job = $derived(doc.library.jobs.find((j) => j.id === draft.job) ?? null);
  const on = $derived(TOOLS.filter((t) => t.optional && isOn(draft.features, t.id)).length);
  const optional = TOOLS.filter((t) => t.optional).length;

  const partsLabel = $derived(draft.parts.length === 1 ? nameOf(draft.parts[0]!.id) : `${draft.parts.length} parts`);
  /** The note beside a part in the list: whether it is on this sheet, and how many copies or paths. */
  function partNote(here: number, copies: number, contours: number): string {
    if (!here) return t('Not on this sheet');
    return copies > 1 ? `${copies} copies · Select` : `${plural(contours, 'path')} · Select`;
  }

  /** Cuts run on the material; a mark among other layers needs its own recipe. */
  const layersMissing = $derived(draft.layers.some((l) => !l.chosen));
  function layerHow(layer: DraftLayer): string {
    const mode = layer.mode === 'mark' ? t('Mark') : 'Cut';
    if (!layer.output) return `${mode} · Off`;
    if (!layer.chosen) return `${mode} · Choose a recipe`;
    return `${mode} · ${layer.recipe ? recipeLabel(layer.recipe) : 'Material'}`;
  }

  let selectedLayer = $state('');
  $effect(() => { if (!draft.layers.some(l => l.name === selectedLayer)) selectedLayer = draft.layers[0]?.name ?? ''; });
  let sheet = $state(false);
  let checklist = $state(false);

  async function run(action: () => Promise<unknown>, then?: () => void): Promise<void> {
    try { await action(); then?.(); } catch (error) { ui.say(explain(error), true); }
  }

  const save = () => saveJob(draft, job, (action) => void run(action));

  function review(dryRun: boolean): void {
    run(async () => { if (draft.dry_run !== dryRun) await api.compile(dryRun); }, () => { ui.tab = 'run'; });
  }
</script>

<div class="job-scroll">
<div class="card2 compact">
  <div class="card2-head">
    <div><h3>{t("Parts")}</h3><span class="muted">{partsLabel}</span></div>
    <div class="parts-actions">
      <button class="btn btn-ghost" onclick={() => beginProject()}>{t("New project")}</button>
      <button class="btn btn-ghost" onclick={() => beginProject('add')}>{t("New sketch")}</button>
      <button class="btn btn-ghost" onclick={() => (texting = true)}>{t("Add text")}</button>
      <button class="btn btn-ghost" onclick={() => (adding = true)}>{t("Add parts")}</button>
    </div>
  </div>
  {#each draft.parts as part (part.id)}<button class="btn btn-ghost block" onclick={() => editPart(part.id)}>{t("Edit drawing in sketch")}: {nameOf(part.id)}</button>{/each}
  {#if draft.parts.length > 1}
    <ul class="job-parts">
      {#each draft.parts as part (part.id)}
        {@const here = draft.placed.filter((p) => p.source >= part.first && p.source < part.first + part.contours).length}
        {@const copies = Math.max(1, Math.round(here / Math.max(1, part.contours)))}
        <li>
          <button class="job-part" disabled={!onselectpart || !here} onclick={() => onselectpart?.(part.first, part.contours)}>
            <span>{nameOf(part.id)}</span>
            <small>{partNote(here, copies, part.contours)}</small>
          </button>
        </li>
      {/each}
    </ul>
    <p class="muted">{tl("Delete a part's shapes on the drawing to take it out of the job.")}</p>
  {/if}
</div>

<div class="card2 compact" class:needs={layersMissing}>
  <button class="mat-row" onclick={() => (sheet = true)}>
    {#if recipe}
      {@const art = pictureOf(recipe.name, recipe.photo)}
      {#if art}<img class="swatch" src={art} alt="">{:else}<span class="swatch" style="background:var(--panel-2)"></span>{/if}
      <span class="mat-text">
        <strong>{recipe.name}</strong>
        <small>{quantity(recipe.thickness_mm, 'mm')} · {tl(recipe.gas)} · {laserLabel(recipe.laser)}</small>
      </span>
    {:else}
      <span class="swatch" style="background:var(--warn-soft)"></span>
      <span class="mat-text"><strong class="warn-text">{t("Choose a material")}</strong><small>{tl("Required before running")}</small></span>
    {/if}
    <span class="link">{t("Change")}</span>
  </button>
  {#if recipe}<MaterialSummary source={recipe} variant="compact" />{/if}
  {#if draft.layers.length}
    <label class="layer-choice">{tl("Machining scope")}
      <select bind:value={selectedLayer}>{#each draft.layers as layer (layer.name)}<option value={layer.name}>{layer.name} · {layerHow(layer)}</option>{/each}</select>
    </label>
    <button class="btn btn-ghost" onclick={() => ui.layerSheet = selectedLayer}>{tl("Open machining settings")}</button>
  {/if}
</div>

{#if draft.calibration}<p class="correction-status">{tl("Correction off")}</p>{:else if draft.correction}<p class="correction-status">{tl("Matrix correction")}</p>{/if}

<div class="card2 compact rows">
  <div class="info-row">
    <span><strong>{tl("Machining")}</strong><small>{tl('{0} of {1} on', on, optional)}{#if draft.feature_source} · {tl('from {0}', draft.feature_source.name)}{/if}</small></span>
    <span><strong>{tl("Preflight")}</strong><small>{preflightLabel(draft)}</small></span>
    <button class="btn btn-ghost" onclick={() => (checklist = true)}>{t("Edit")}</button>
  </div>
</div>

<div class="stats">
  <div><b>{displayNumber(draft.preview?.length_mm, 'mm', 0)}</b><span>{tl('{0} cut', unitLabel('mm'))}</span></div>
  <div><b>{draft.compiled ? draft.compiled.pierces.length : '—'}</b><span>{tl("pierces")}</span></div>
  <div><b>{draft.compiled ? seconds(draft.compiled.seconds) : '—'}</b><span>{tl("est. time")}</span></div>
</div>
{#if draft.compiled?.plan.some((pass) => pass.omitted_cooling > 0)}
  <p class="muted">{tl('{0} cooling points skipped within {1} of endpoints.', draft.compiled.plan.reduce((n, pass) => n + pass.omitted_cooling, 0), quantity(0.2, 'mm'))}</p>
{/if}
{#if draft.error}<div class="warn-text draft-error">{plain(draft.error).text}</div>{/if}
{#if draft.compiled}<GasLaserCard />{/if}
</div>

<div class="stack">
  <div class="row">
    <button class="btn btn-ghost lg" onclick={save} disabled={!recipe}>{saveLabel(draft)}</button>
    <button class="btn btn-ghost lg" onclick={() => review(true)} disabled={!doc.readiness.compile.ok} title={doc.readiness.compile.reason ?? ''}>{tl("Dry run")}</button>
  </div>
  <button class="btn btn-primary xl block" onclick={() => review(false)} disabled={!doc.readiness.compile.ok} title={doc.readiness.compile.reason ?? ''}>{tl("Go to Run →")}</button>
</div>

{#if sheet}<MaterialSheet onclose={() => (sheet = false)} />{/if}
{#if adding}<AddParts onclose={() => (adding = false)} />{/if}
{#if texting}<CreateText onclose={() => (texting = false)} />{/if}
{#if checklist}<PreflightEditor scope="job" onclose={() => (checklist = false)} />{/if}

<style>
  /* The cards scroll on a short screen; saving and Go to Run stay in reach. */
  .parts-actions { display:flex; gap:8px; flex-wrap:wrap; justify-content:flex-end; }
  .job-scroll { flex:1; min-height:0; overflow-y:auto; overflow-x:hidden; overscroll-behavior:contain; display:flex; flex-direction:column; gap:8px; margin:-2px; padding:2px; }
  .stack { flex-shrink:0; }
  .correction-status { padding:10px 12px; font-size:var(--t-sm); color:var(--ink-3); border-left:2px solid var(--accent); }
  .job-parts { list-style:none; margin:0; padding:0; display:grid; gap:6px; max-height:220px; overflow-y:auto; }
  .job-part {
    width:100%; min-height:48px; display:flex; align-items:center; justify-content:space-between; gap:12px; padding:8px 12px;
    border:1px solid var(--line); border-radius:10px; background:var(--panel-2); color:var(--ink); text-align:left; cursor:pointer;
  }
  .job-part span { font-weight:600; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .job-part small { flex:none; color:var(--ink-3); font-size:var(--t-sm); }
  .compact { min-width:0; padding:10px 12px; gap:8px; }
  .mat-row { display:flex; align-items:center; gap:12px; min-height:52px; padding:0; border:0; background:transparent; color:var(--ink); font:inherit; text-align:left; cursor:pointer; }
  .mat-row .swatch { flex:none; width:40px; height:40px; border-radius:8px; object-fit:cover; }
  .mat-text { flex:1; min-width:0; display:grid; gap:2px; }
  .mat-text strong { font-size:var(--t-base); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .mat-text small { font-size:var(--t-sm); color:var(--ink-3); }
  .draft-error { font-size:var(--t-sm); }
  .rows { gap:0; padding-top:4px; padding-bottom:4px; }
  .info-row { display:flex; align-items:center; justify-content:space-between; gap:10px; min-height:44px; }
  .info-row > span { flex:1; display:grid; gap:2px; min-width:0; }
  .info-row strong { font-size:var(--t-base); }
  .info-row small { font-size:var(--t-sm); color:var(--ink-3); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .info-row .btn { min-height:44px; }
  .compact.needs { border-color:var(--warn); }
  .layer-choice { display:grid; gap:6px; margin-top:8px; }
  .layer-choice select { width:100%; min-width:0; min-height:44px; }
</style>
