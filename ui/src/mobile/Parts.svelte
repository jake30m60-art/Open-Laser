<script lang="ts">
  import { listRemote, type RemoteSummary } from '../sketch/remote';
  import { sketchStore } from '../sketch/store.svelte';
  import { beginProject } from '../lib/sketch-workflow';
  import { explain } from '../lib/format';
  import { t } from '../i18n/index';

  import SkippedFiles from '../components/SkippedFiles.svelte';
  import { access } from '../lib/access.svelte';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { fuzzyScore } from '../lib/search';
  import { recipeLabel, size } from '../lib/format';
  import Preview from './Preview.svelte';
  import Modal from '../components/Modal.svelte';
  import ImportParts from '../components/ImportParts.svelte';
  import SheetLibrary from '../components/SheetLibrary.svelte';
  import EditHistory from '../components/EditHistory.svelte';
  import PartsSide from '../routes/PartsSide.svelte';
  import { api } from '../api/client';
  import { plural } from '../lib/format';
  import { livePicks, togglePick } from '../lib/job-parts';
  import { withBusy } from '../lib/busy';

  let kind = $state<'all' | 'parts' | 'jobs' | 'sheets' | 'sketches'>('all');
  let sketches = $state<RemoteSummary[]>([]);
  let sketchError = $state('');
  let sketchLoading = $state(false);
  $effect(() => {
    if (kind !== 'sketches') return;
    let active = true;
    sketchLoading = true;
    sketchError = '';
    void listRemote().then(result => { if (active) { if (result) sketches = result; else sketchError = t('Could not load sketches.'); } })
      .catch(error => { if (active) sketchError = explain(error); })
      .finally(() => { if (active) sketchLoading = false; });
    return () => { active = false; };
  });
  async function openSketch(id: string): Promise<void> {
    try {
      await sketchStore.sync();
      if (!await sketchStore.openDocument(id)) { ui.say(t('Could not load sketches.'), true); return; }
      ui.tab = 'sketch';
    } catch (error) { ui.say(explain(error), true); }
  }
  let search = $state('');
  let actions = $state(false);
  let history = $state(false);
  let favorites = $state(false);
  let details = $state(false);
  const doc = $derived(server.doc!);
  const library = $derived(doc.library);
  const items = $derived([
    ...library.parts.map(item => ({ item, kind:'parts' as const })),
    ...library.jobs.map(item => ({ item, kind:'jobs' as const })),
  ]
    .filter(entry => (kind === 'all' || entry.kind === kind) && (!favorites || entry.item.favourite))
    .map(entry => {
      const { item } = entry;
      const text = `${item.name} ${item.notes} ${item.tags.join(' ')} ${'recipe' in item ? recipeLabel(item.recipe) : ''}`;
      return { ...entry, score: search.trim() ? fuzzyScore(text, search.trim()) : 1 };
    })
    .filter(entry => entry.score > 0)
    .sort((a,b) => b.score - a.score
      || Number(b.item.favourite) - Number(a.item.favourite)
      || b.item.updated - a.item.updated));
  const blocked = $derived(!access.canControl || !!doc.machine.operation || !server.link);

  // Picking parts to set up as one job, in the order picked.
  const picking = $derived(ui.partPicks !== null);
  const picks = $derived(livePicks(ui.partPicks ?? [], library.parts));
  const inDraft = $derived(doc.draft ? picks.some((id) => doc.draft!.parts.some((p) => p.id === id)) : false);
  let busy = $state(false);
  $effect(() => { if (picking) details = false; });
  function choose(id: string, type: 'parts' | 'jobs'): void {
    if (!picking) {
      ui.selected = id;
      details = true;
      return;
    }
    if (type === 'jobs') {
      ui.say(t("A saved job cuts its own parts. Pick parts to set up a new job."));
      return;
    }
    ui.partPicks = togglePick(picks, id);
  }
  async function finish(add: boolean): Promise<void> {
    if (busy || !picks.length) return;
    await withBusy((b) => (busy = b), async () => {
      await (add ? api.addParts(picks) : api.openParts(picks));
      const n = picks.length;
      ui.partPicks = null;
      ui.tab = 'setup';
      const opened = n > 1 ? t('{0} side by side · Nest parts fills a sheet', plural(n, 'part')) : t('Part opened');
      ui.say(add ? t('Added {0} beside the sheet · Undo takes them off', plural(n, 'part')) : opened);
    });
  }
  const picksNote = $derived(
    picks.length ? t('{0} picked · laid out in this order', plural(picks.length, 'part')) : t('Tap the parts to cut together.'),
  );
  const setupLabel = $derived(picks.length > 1 ? t('Set up job with {0}', plural(picks.length, 'part')) : t('Set up job'));
  function toggleFavorites(): void {
    actions = false;
    favorites = !favorites;
    kind = 'all';
  }
  function showHistory(): void {
    actions = false;
    history = true;
  }
</script>

<div class="phone-page-title">
  <h1>{t("Parts")}</h1>
  <div class="phone-title-actions">
    <button class="btn btn-ghost pick-toggle" aria-pressed={picking} disabled={blocked} onclick={() => (ui.partPicks = picking ? null : [])}>
      {picking ? t("Cancel") : t('Pick parts')}
    </button>
    <button class="phone-icon" aria-label={t("Library actions")} onclick={() => actions = true}><i class="ic ic-plus"></i></button>
  </div>
</div>
<SkippedFiles />
<div class="phone-segments" role="group" aria-label={t("Library type")}>
  {#each [['all','All'],['parts','Parts'],['jobs','Jobs'],['sketches','Sketches'],['sheets','Sheets']] as [id,label]}
    <button aria-pressed={kind === id} onclick={() => kind = id as typeof kind}>{t(label)}</button>
  {/each}
</div>
{#if kind === 'sheets'}
  <SheetLibrary />
{:else if kind === 'sketches'}
  <label class="phone-search"><input type="search" aria-label={t('Search sketches')} placeholder={t('Search sketches')} bind:value={search} /></label>
  {#if sketchError}<p role="alert">{sketchError}</p>
  {:else if sketchLoading}<p role="status">{t('Loading…')}</p>
  {:else}
    <div class="phone-library-list">
      {#each sketches.filter(s => !search.trim() || fuzzyScore(s.name, search.trim()) > 0) as sketch (sketch.id)}
        <button class="phone-library-item" onclick={() => openSketch(sketch.id)}>
          <Preview outline={sketch.outline} label={sketch.name} small />
          <span><strong>{sketch.name}</strong><small>{t('Edit as sketch')}</small></span>
          <i class="ic ic-arrow-right"></i>
        </button>
      {:else}<p class="phone-empty">{t(search ? 'Nothing matches' : 'No sketches yet')}</p>{/each}
    </div>
  {/if}
{:else}
  <label class="phone-search">
    <i class="ic ic-search"></i>
    <input type="search" aria-label={t("Search parts and jobs")} placeholder={t("Search parts and jobs")} bind:value={search} />
  </label>
  {#if favorites}<div class="phone-filter-note">{t("Favorites")} <button onclick={() => favorites = false}>{t("Clear")}</button></div>{/if}
  <div class="phone-library-list">
    {#each items as { item, kind: type } (item.id)}
      {@const pick = picking && type === 'parts' ? picks.indexOf(item.id) : -1}
      <button
        class="phone-library-item"
        class:picked={pick >= 0}
        class:unpickable={picking && type === 'jobs'}
        disabled={blocked}
        aria-pressed={picking && type === 'parts' ? pick >= 0 : undefined}
        onclick={() => choose(item.id, type)}>
        {#if 'has_image' in item && item.has_image}
          <img class="part-image" src={`${api.partImageUrl(item.id)}?v=${item.updated}`} alt={item.name} loading="lazy" />
        {:else}<Preview outline={item.outline} small />{/if}
        <span>
          <strong>{item.name}</strong>
          <small>{type === 'jobs' && 'recipe' in item ? recipeLabel(item.recipe) : size(item.bounds)}</small>
        </span>
        {#if picking && type === 'parts'}
          <span class="pick-mark" class:on={pick >= 0} aria-hidden="true">{pick >= 0 ? pick + 1 : ''}</span>
        {:else}
          <i class="ic ic-arrow-right"></i>
        {/if}
      </button>
    {:else}
      <div class="phone-empty">
        <i class="ic ic-folder"></i>
        <h2>{search ? t('Nothing matches') : t('No parts or jobs yet')}</h2>
        <p>{search ? t('Try a name or material.') : t('Import a drawing to set up your first job.')}</p>
        {#if !search}<button class="btn btn-primary" onclick={() => actions = true}>{t("Import a drawing")}</button>{/if}
      </div>
    {/each}
  </div>
{/if}

{#if picking}
  <div class="phone-picks-bar">
    <p>{picksNote}</p>
    {#if doc.draft}
      <button class="btn btn-ghost" disabled={busy || blocked || !picks.length || inDraft} onclick={() => finish(true)}>{t("Add to")} {doc.draft.name}</button>
      {#if inDraft}<p class="gate-reason">{t('A picked part is already in {0}.', doc.draft.name)}</p>{/if}
    {/if}
    <button class="phone-primary" disabled={busy || blocked || !picks.length} onclick={() => finish(false)}>
      {setupLabel}<i class="ic ic-arrow-right"></i>
    </button>
  </div>
{/if}

{#if actions}<Modal title={t("Library actions")} onclose={() => actions = false}>
  <div class="phone-import-actions"><ImportParts /></div>
  <div class="phone-action-list">
    <button onclick={() => { actions = false; beginProject(); }}>{t("New project")}<i class="ic ic-arrow-right"></i></button>
    <button onclick={toggleFavorites}>{t("Favorites")}<i class="ic ic-arrow-right"></i></button>
    <button onclick={showHistory}>{t("History")}<i class="ic ic-arrow-right"></i></button>
  </div>
</Modal>{/if}
{#if history}<EditHistory onclose={() => history = false} />{/if}
{#if details}<Modal title={t("Part details")} onclose={() => details = false}><div class="phone-part-details"><PartsSide /></div></Modal>{/if}

<style>
  .part-image { width:64px; height:64px; flex:none; object-fit:contain; background:white; border-radius:12px; }
  .phone-title-actions { display:flex; align-items:center; gap:8px; }
  .pick-toggle { min-height:44px; }
  .pick-toggle[aria-pressed="true"] { border-color:var(--accent); color:var(--accent); }
  .pick-mark { flex:none; width:30px; height:30px; border-radius:50%; border:2px solid var(--ink-3); display:grid; place-items:center; font-weight:700; font-size:var(--t-sm); }
  .pick-mark.on { background:var(--accent); border-color:var(--accent); color:#fff; }
  .phone-library-item.picked { border-color:var(--accent); background:var(--accent-soft); }
  .phone-library-item.unpickable { opacity:.45; }
  .phone-picks-bar { position:sticky; bottom:0; margin-top:12px; padding:12px; display:grid; gap:8px; background:var(--panel); border:1px solid var(--line); border-radius:16px; box-shadow:0 -6px 18px #17230d14; }
  .phone-picks-bar p { margin:0; font-size:var(--t-sm); color:var(--ink-3); }
  .phone-picks-bar .btn { min-height:48px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .phone-picks-bar .phone-primary { background:var(--accent); }
</style>
