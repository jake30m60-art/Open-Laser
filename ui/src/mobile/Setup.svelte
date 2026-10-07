<script lang="ts">
  import { t as tl } from '../i18n/index';
  import { t } from '../i18n/index';

  import { api } from '../api/client';
  import { access } from '../lib/access.svelte';
  import { server } from '../stores/server.svelte';
  import { ui, type FeatureId } from '../stores/ui.svelte';
  import { preflightLabel, saveJob, saveLabel } from '../lib/save-job';
  import { laserLabel, recipeLabel, size } from '../lib/format';
  import { plain } from '../lib/plain';
  import { TOOLS, isOn, stateOf } from '../lib/features';
  import Preview from './Preview.svelte';
  import Modal from '../components/Modal.svelte';
  import PreflightEditor from '../components/PreflightEditor.svelte';
  import { onlyPart } from '../lib/job-parts';
  import AddParts from '../components/AddParts.svelte';
  import CreateText from '../components/CreateText.svelte';
  import MaterialSummary from '../components/MaterialSummary.svelte';
  import { withBusy } from '../lib/busy';

  let { edit }: { edit: (tool:FeatureId | 'copy' | 'nest' | 'layout') => void } = $props();
  const doc = $derived(server.doc!);
  const draft = $derived(doc.draft);
  const part = $derived(onlyPart(draft, doc.library.parts));
  const job = $derived(doc.library.jobs.find(j => j.id === draft?.job));
  const outline = $derived(
    draft?.preview?.contours.flatMap(c => c.paths.filter(p => p.kind === 'cut').map(p => p.points)) ?? part?.outline ?? [],
  );
  const active = $derived(draft ? TOOLS.filter(t => t.optional && isOn(draft.features,t.id)).length : 0);
  let busy = $state(false);
  let material = $state(false);
  let search = $state('');
  let tools = $state(false);
  let preflight = $state(false);
  let adding = $state(false);
  let texting = $state(false);
  const blocked = $derived(!access.canControl || busy || !!doc.machine.operation || !server.link);
  const recipes = $derived(doc.library.recipes
    .filter(r => recipeLabel(r).toLocaleLowerCase().includes(search.toLocaleLowerCase()))
    .sort((a,b) => Number(b.laser === doc.mode) - Number(a.laser === doc.mode)
      || Number(b.favourite) - Number(a.favourite)
      || a.name.localeCompare(b.name)));
  const materialName = $derived(draft?.recipe ? recipeLabel(draft.recipe) : t('Choose a material'));
  const partsHint = $derived(
    draft?.parts.length === 1 ? t('1 part · Add more to cut them together') : t('{0} parts side by side · Add more', draft?.parts.length),
  );
  async function perform(action: () => Promise<unknown>): Promise<void> {
    if (blocked) return;
    await withBusy((b) => (busy = b), action);
  }
  function review(dryRun:boolean): void {
    void perform(async () => {
      if (draft?.dry_run !== dryRun) await api.compile(dryRun);
      ui.tab = 'run';
    });
  }
  const save = () => { if (draft) saveJob(draft, job, (action) => void perform(action)); };
  function pickRecipe(id: string): void {
    void perform(async () => {
      await api.setRecipe(id);
      material = false;
    });
  }
  function openTool(tool: FeatureId | 'copy' | 'nest'): void {
    tools = false;
    edit(tool);
  }
</script>

<div class="phone-run phone-setup-home">
  <div class="phone-run-scroll">
    <div class="phone-page-title"><h1>{t("Setup")}</h1></div>
    {#if draft}
      <section class="phone-job-card">
        <div class="phone-job-heading"><div><h2>{job?.name ?? (draft.name || t('Current job'))}</h2></div></div>
        <Preview {outline} />
        <div class="phone-job-spec">
          <span>{size(draft.preview?.bounds ?? part?.bounds ?? null)}</span>
          <button class="phone-text-action" disabled={blocked} onclick={() => edit('layout')}>{tl("Edit layout")}</button>
        </div>
      </section>
      <button class="phone-material" disabled={blocked} onclick={() => material = true}>
        <i class="ic ic-layers"></i>
        <span>
          <small>{tl("Material")}</small><strong>{materialName}</strong>
        </span>
        <i class="ic ic-arrow-right"></i>
      </button>
      <div class="phone-action-list phone-setup-options">
        <button disabled={blocked} onclick={() => adding = true}>
          <span>{t("Parts")}<small>{partsHint}</small></span><i class="ic ic-plus"></i>
        </button>
        <button disabled={blocked} onclick={() => texting = true}>
          <span>{t("Add text")}<small>{tl("Letters welded into cutting outlines")}</small></span><i class="ic ic-plus"></i>
        </button>
        <button disabled={blocked} onclick={() => tools = true}>
          <span>{tl("Machining")}<small>{active ? t("{0} tools on", active) : t('Default settings')}</small></span><i class="ic ic-arrow-right"></i>
        </button>
        <button disabled={blocked} onclick={() => preflight = true}>
          <span>{tl("Preflight")}<small>{draft ? preflightLabel(draft) : ''}</small></span><i class="ic ic-arrow-right"></i>
        </button>
      </div>
    {:else}
      <div class="phone-empty">
        <i class="ic ic-folder"></i>
        <h2>{tl("No part open")}</h2>
        <p>{t("Choose a part or saved job to set up, or start from text.")}</p>
        <button class="phone-primary" onclick={() => ui.tab = 'parts'}>{tl("Go to Parts")}</button>
        <button class="phone-text-action" onclick={() => texting = true}>{t("Add text")}</button>
      </div>
    {/if}
  </div>
  {#if draft}<div class="phone-run-footer">
    <div class="phone-secondary-actions">
      <button class="btn btn-ghost" disabled={blocked || !draft.recipe} onclick={save}>{saveLabel(draft)}</button>
      <button class="btn btn-ghost" disabled={blocked || !doc.readiness.compile.ok} onclick={() => review(true)}>{tl("Dry run")}</button>
    </div>
    <button class="phone-primary" disabled={blocked || !doc.readiness.compile.ok} onclick={() => review(false)}>
      {busy ? t('Preparing…') : t('Go to Run')}<i class="ic ic-arrow-right"></i>
    </button>
    {#if !doc.readiness.compile.ok}<p class="phone-readiness">{plain(doc.readiness.compile.reason).text}</p>{/if}
  </div>{/if}
</div>

{#if material}<Modal title={tl("Material")} onclose={() => material = false}>
  <div class="phone-material-picker">
    <input type="search" aria-label={tl("Search materials")} placeholder={tl("Search materials")} bind:value={search} />
    {#each recipes as recipe}
      <button disabled={busy} onclick={() => pickRecipe(recipe.id)}>
        <strong>{recipeLabel(recipe)}</strong>
        <small>{laserLabel(recipe.laser)}</small>
        <MaterialSummary source={recipe} variant="line" />
      </button>
    {:else}
      <p>{tl("No matching materials.")}</p>
    {/each}
  </div>
</Modal>{/if}
{#if tools}<Modal title={tl("Machining")} onclose={() => tools = false}>
  <div class="phone-action-list">
    {#each TOOLS as tool}
      <button onclick={() => openTool(tool.id as FeatureId)}>
        <span>{t(tool.name)}<small>{draft ? stateOf(draft.features,tool.id) : ''}</small></span><i class="ic ic-arrow-right"></i>
      </button>
    {/each}
    <button onclick={() => openTool('nest')}>{t("Nest parts")}<i class="ic ic-arrow-right"></i></button>
    <button onclick={() => openTool('copy')}>{t("Copy machining from job")}<i class="ic ic-arrow-right"></i></button>
  </div>
</Modal>{/if}
{#if preflight}<PreflightEditor scope="job" onclose={() => preflight = false} />{/if}
{#if adding}<AddParts onclose={() => adding = false} />{/if}
{#if texting}<CreateText onclose={() => texting = false} />{/if}
