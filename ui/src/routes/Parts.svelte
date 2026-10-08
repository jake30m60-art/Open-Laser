<script lang="ts">
  import { t } from '../i18n/index';

  import { beginProject, editPart } from '../lib/sketch-workflow';
  import { sketchStore } from '../sketch/store.svelte';
  import { listRemote, outline as sketchOutline, type RemoteSummary } from '../sketch/remote';
  import SkippedFiles from '../components/SkippedFiles.svelte';
  import SheetLibrary from '../components/SheetLibrary.svelte';
  import StockEditor from '../components/StockEditor.svelte';
  import { sheetLabel, boundsLabel } from '../lib/sheet-sizes';
  import ImportParts from '../components/ImportParts.svelte';
  import EditHistory from '../components/EditHistory.svelte';
  import PartsSide from './PartsSide.svelte';
  import PartPicks from '../components/PartPicks.svelte';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { osk } from '../lib/osk.svelte';
  import { fuzzyScore } from '../lib/search';
  import { folderPath as pathOfFolder } from '../lib/folders';
  import { explain, laserLabel, plural, recipeLabel, size } from '../lib/format';
  import { quantity as quantityOf } from '../lib/units.svelte';
  import { boxOf, pathOf, viewBoxFor } from '../lib/svg';
  import { hasAllParts, togglePick } from '../lib/job-parts';
  import type { Folder, JobView } from '../api';

  const doc = $derived(server.doc!);
  const library = $derived(doc.library);

  /** A card: a part, a saved job, or sheets kept in a folder. */
  type Card = {
    id: string; kind: 'part' | 'job' | 'stock' | 'remnant' | 'sketch'; name: string; search: string; folder: string | null; laser: 'fiber' | 'co2' | null;
    material: string; size: string; contours: number; favourite: boolean; updated: number; outline: number[][][]; parts: number; image?: string;
  };

  let kindFilter = $state<'all' | 'part' | 'job' | 'sheet' | 'sketch'>('all');
  /** The server's sketches, read each time the Sketches tab opens. */
  let remoteSketches = $state<RemoteSummary[] | null>(null);
  $effect(() => { if (kindFilter === 'sketch') void listRemote().then((r) => { remoteSketches = r; }); });
  const sketchCards = $derived.by((): Card[] => {
    const from = remoteSketches ?? sketchStore.documents.filter((d) => !d.remote).map((d) => ({ id: d.id, name: d.name, outline: sketchOutline(d.sketch), updated: d.updated ?? 0 }));
    return from.map((s) => {
      const box = boxOf(s.outline);
      return {
        id: s.id, kind: 'sketch', name: s.name, search: '', folder: null, laser: null, material: '',
        size: box ? `${quantityOf(box.maxX - box.minX, 'mm')} × ${quantityOf(box.maxY - box.minY, 'mm')}` : t('Empty sketch'),
        contours: 0, favourite: false, updated: s.updated, outline: s.outline, parts: 0,
      };
    });
  });
  /** Opens a sketch to edit; one only the server has is fetched first. */
  async function openSketch(id: string): Promise<void> {
    if (!sketchStore.documents.some((d) => d.id === id)) await sketchStore.sync();
    if (!await sketchStore.openDocument(id)) { ui.say(t('Could not load sketches.'), true); return; }
    ui.tab = 'sketch';
  }
  let laserFilter = $state<'all' | 'fiber' | 'co2'>('all');
  let favourites = $state(false);
  let filterOpen = $state(false);
  let filterMenu = $state<HTMLDivElement | null>(null);
  const LASERS = [['all', t("All lasers")], ['fiber', t("Fiber")], ['co2', 'CO₂']] as const;
  const laserName = $derived(LASERS.find(([k]) => k === laserFilter)![1]);
  const filtered = $derived(laserFilter !== 'all' || favourites);
  const filterName = $derived(laserFilter !== 'all' ? laserName : t("Filter"));

  // The filter menu closes on a tap outside it or on Escape.
  $effect(() => {
    if (!filterOpen) return;
    const away = (e: PointerEvent) => { if (filterMenu && !filterMenu.contains(e.target as Node)) filterOpen = false; };
    const escape = (e: KeyboardEvent) => { if (e.key === 'Escape') filterOpen = false; };
    document.addEventListener('pointerdown', away);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('pointerdown', away);
      document.removeEventListener('keydown', escape);
    };
  });
  let page = $state(0);
  let historyOpen = $state(false);

  const cards = $derived.by((): Card[] => {
    const parts = library.parts.map((p): Card => ({
      id: p.id, kind: 'part', name: p.name, search: `${p.tags.join(' ')} ${p.notes}`, folder: p.folder, laser: null, material: '',
      size: size(p.bounds), contours: p.contours, favourite: p.favourite, updated: p.updated, outline: p.outline, parts: 1,
      image: p.has_image ? `${api.partImageUrl(p.id)}?v=${p.updated}` : undefined,
    }));
    const jobs = library.jobs.flatMap((j: JobView): Card[] => {
      if (!hasAllParts(j, library.parts)) return [];
      return [{
        id: j.id, kind: 'job', name: j.name, search: `${j.tags.join(' ')} ${j.notes}`, folder: j.folder, laser: j.recipe.laser,
        material: recipeLabel(j.recipe), size: size(j.bounds), contours: j.contours, favourite: j.favourite, updated: j.updated,
        outline: j.outline, parts: j.parts.length,
      }];
    });
    // Sheets show beside the jobs they are kept for; the Stock tab lists them all.
    const box = (w: number, h: number) => [[[0, 0], [w, 0], [w, h], [0, h], [0, 0]]];
    const stock = library.stock.filter((i) => i.folder).map((i): Card => ({
      id: i.id, kind: 'stock', name: `${i.material} · ${plural(i.quantity, 'sheet')}`, search: i.material, folder: i.folder,
      laser: i.laser, material: quantityOf(i.thickness_mm, 'mm'), size: sheetLabel(i.width_mm, i.height_mm), contours: 0,
      favourite: false, updated: 0, outline: box(i.width_mm, i.height_mm), parts: 0,
    }));
    const remnants = library.remnants.filter((r) => r.folder).map((r): Card => ({
      id: r.id, kind: 'remnant', name: r.name, search: r.material, folder: r.folder, laser: r.mode,
      material: `${r.material} · ${quantityOf(r.thickness_mm, 'mm')}`,
      size: boundsLabel(r.bounds), contours: 0, favourite: false,
      updated: r.at, outline: [r.outline, ...r.cutouts], parts: 0,
    }));
    return kindFilter === 'sketch' ? sketchCards : [...parts, ...jobs, ...stock, ...remnants];
  });
  let editingStock = $state<string | null>(null);

  const folderPath = (id: string | null) => pathOfFolder(library.folders, id);
  /** The folder names down to `id`, joined with `separator`. */
  const folderTrail = (id: string | null, separator: string) => folderPath(id).map((f) => f.name).join(separator);

  const q = $derived(ui.search.trim());
  // A search and the favourites look in every folder; otherwise the open folder.
  const everywhere = $derived(!!q || favourites);
  const visible = $derived.by(() => {
    let items = cards.filter((c) =>
      (kindFilter === 'all' || c.kind === kindFilter)
      && (!favourites || c.favourite)
      && (laserFilter === 'all' || c.laser === laserFilter || (c.laser === null && kindFilter !== 'job')));
    if (q) {
      return items
        .map((c) => [fuzzyScore(`${c.name} ${c.search} ${c.material} ${c.size} ${folderTrail(c.folder, ' ')}`, q), c] as const)
        .filter((x) => x[0] > 0)
        .sort((a, b) => b[0] - a[0])
        .map((x) => x[1]);
    }
    // Sketches have no folders: they all show together.
    return favourites || kindFilter === 'sketch' ? items : items.filter((c) => (c.folder ?? null) === ui.folder);
  });
  const folders = $derived(
    q || kindFilter === 'sketch' ? []
    : favourites ? library.folders.filter((f) => f.favourite)
    : library.folders.filter((f) => (f.parent ?? null) === ui.folder).sort((a, b) => Number(b.favourite) - Number(a.favourite)),
  );

  // Cards never scroll: as many as fit, the rest on further pages.
  let grid = $state<HTMLDivElement | null>(null);
  let per = $state(8);
  $effect(() => {
    if (!grid) return;
    const measure = () => {
      const rect = grid!.getBoundingClientRect();
      const cols = Math.max(1, Math.floor((rect.width - 44 + 12) / (210 + 12)));
      const rows = Math.max(1, Math.floor((rect.height - 24 + 12) / (172 + 12)));
      per = cols * rows;
      grid!.style.gridTemplateColumns = `repeat(${cols}, minmax(0, 1fr))`;
      // Rows share the spare height, so previews grow instead of leaving a gap.
      grid!.style.gridAutoRows = `${Math.max(172, Math.floor((rect.height - 24 - (rows - 1) * 12) / rows))}px`;
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(grid);
    return () => observer.disconnect();
  });
  const total = $derived(folders.length + visible.length);
  const pages = $derived(Math.max(1, Math.ceil(total / per)));
  $effect(() => { if (page > pages - 1) page = pages - 1; });
  const shown = $derived.by(() => {
    const all: Array<{ folder: Folder } | { card: Card }> = [
      ...folders.map((folder) => ({ folder })),
      ...visible.map((card) => ({ card })),
    ];
    return all.slice(page * per, page * per + per);
  });

  const thumb = (outline: number[][][]) => {
    const box = boxOf(outline);
    return box ? viewBoxFor(box, 100, 70) : '0 0 100 70';
  };

  // Picking parts to set up together: a tap picks or unpicks a part card;
  // folders still open, so parts can be picked from several.
  const picking = $derived(ui.partPicks !== null);
  const shownParts = $derived(visible.filter((c) => c.kind === 'part').map((c) => c.id));
  function choose(card: Card): void {
    if (card.kind === 'stock' || card.kind === 'remnant') { editingStock = card.id; return; }
    if (card.kind === 'sketch') { void openSketch(card.id); return; }
    if (!picking) { ui.selected = card.id; return; }
    if (card.kind === 'job') {
      ui.say(t("A saved job cuts its own parts. Pick parts to set up a new job."));
      return;
    }
    ui.partPicks = togglePick(ui.partPicks ?? [], card.id);
  }

  async function star(card: Card): Promise<void> {
    try {
      await (card.kind === 'job'
        ? api.updateJob(card.id, { favourite: !card.favourite })
        : api.updatePart(card.id, { favourite: !card.favourite }));
    } catch (error) {
      ui.say(explain(error), true);
    }
  }

  function newFolder(): void {
    osk.text('New folder name', '', async (name) => {
      if (!name.trim()) return;
      try { await api.addFolder(name.trim(), ui.folder); } catch (error) { ui.say(explain(error), true); }
    });
  }

  function clearFilter(): void {
    laserFilter = 'all';
    favourites = false;
  }

  function clearSearchOnEscape(e: KeyboardEvent): void {
    if (e.key === 'Escape') ui.search = '';
  }

  function openFolder(folder: Folder): void {
    ui.folder = folder.id;
    ui.search = '';
  }

  function starFolder(e: MouseEvent, folder: Folder): void {
    e.stopPropagation();
    api.updateFolder(folder.id, { favourite: !folder.favourite }).catch((error) => ui.say(explain(error), true));
  }

  function starCard(e: MouseEvent, card: Card): void {
    e.stopPropagation();
    star(card);
  }

  /** The kind tag on a card: a job says how many parts it cuts. */
  const kindTag = (c: Card): string =>
    c.kind === 'job' ? (c.parts > 1 ? `Job · ${plural(c.parts, 'part')}` : t("Job"))
    : c.kind === 'stock' ? t("Sheets") : c.kind === 'remnant' ? t("Remnant") : c.kind === 'sketch' ? t('Sketch') : t('Part');

  const emptyTitle = $derived(q || filtered ? t('Nothing matches') : t("Empty folder"));
  const emptyHint = $derived(
    q ? t('Try a shorter word, a material, a size, or a folder name.')
    : filtered ? t('Clear the filter to see everything.')
    : t("Import DXF or SVG, or move parts here."),
  );
</script>

<section class="panel main" class:sheet-library-page={kindFilter === 'sheet'}>
  <div class="toolbar head-row parts-head">
    <h1>{t("Parts")}</h1>
    {#if kindFilter !== 'sheet'}<div class="parts-actions">
      <button class="btn btn-ghost" class:on={picking} aria-pressed={picking} onclick={() => (ui.partPicks = picking ? null : [])}>
        <i class="ic ic-check"></i>{t("Pick parts")}
      </button>
      <button class="btn btn-ghost" onclick={newFolder}><i class="ic ic-folder-plus"></i>{t("Folder")}</button>
      <button class="btn btn-ghost" onclick={() => beginProject()}>{t("New project")}</button>
      <ImportParts />
    </div>{/if}
  </div>
  <div class="toolbar parts-filters">
    <div class="seg" role="group" aria-label={t("Library type")}>
      {#each [['all', 'All'], ['part', 'Parts'], ['job', 'Jobs'], ['sketch', 'Sketches'], ['sheet', 'Sheets']] as [k, label]}
        <button class:on={kindFilter === k} aria-pressed={kindFilter === k} onclick={() => (kindFilter = k as typeof kindFilter)}>{t(label)}</button>
      {/each}
    </div>
    {#if kindFilter !== 'sheet'}
      <div class="filter-menu" bind:this={filterMenu}>
        <button
          class="btn btn-ghost filter-btn"
          class:on={filtered}
          aria-haspopup="true"
          aria-expanded={filterOpen}
          aria-label="Filter: {laserName}{favourites ? ', favorites only' : ''}"
          onclick={() => (filterOpen = !filterOpen)}>
          {filterName}{#if favourites}<i class="ic ic-star-fill"></i>{/if}<i class="ic ic-chev-down"></i>
        </button>
        {#if filterOpen}<div class="filter-pop" role="group" aria-label={t("Filter")}>
          <span class="filter-label">{t("Laser")}</span>
          <div class="seg block">
            {#each LASERS as [k, label]}
              <button class:on={laserFilter === k} aria-pressed={laserFilter === k} onclick={() => (laserFilter = k)}>
                {k === 'all' ? t("All") : label}
              </button>
            {/each}
          </div>
          <button class="btn btn-ghost block fav-toggle" class:on={favourites} aria-pressed={favourites} onclick={() => (favourites = !favourites)}>
            <i class="ic {favourites ? 'ic-star-fill' : 'ic-star'}"></i>{t("Favorites only")}
          </button>
          {#if filtered}<button class="btn btn-ghost block" onclick={clearFilter}>{t("Clear filter")}</button>{/if}
        </div>{/if}
      </div>
      <label class="search">
        <i class="ic ic-search"></i>
        <input
          type="search"
          aria-label={t("Search parts and jobs")}
          placeholder={t("Search parts and jobs")}
          bind:value={ui.search}
          onkeydown={clearSearchOnEscape}>
      </label>
    {/if}
  </div>

  <SkippedFiles />
  {#if kindFilter === 'sheet'}<SheetLibrary />{:else}
  <div class="crumbs">
    {#if q}
      <span class="muted">{t("Results everywhere for")}</span> <strong>“{q}”</strong>
    {:else if favourites}
      <span class="muted">{t("Favorites in every folder")}</span>
    {:else}
      <button class="crumb-btn" onclick={() => (ui.folder = null)}>{t("All parts")}</button>
      {#each folderPath(ui.folder) as f}
        <i class="ic ic-chev-right sm"></i><button class="crumb-btn" onclick={() => (ui.folder = f.id)}>{f.name}</button>
      {/each}
    {/if}
  </div>

  <div class="grid parts-grid" bind:this={grid}>
    {#each shown as item}
      {#if 'folder' in item}
        {@const n = cards.filter((c) => folderPath(c.folder).some((f) => f.id === item.folder.id)).length}
        <div
          class="card folder"
          role="button"
          tabindex="0"
          onclick={() => openFolder(item.folder)}
          onkeydown={(e) => { if (e.key === 'Enter') { ui.folder = item.folder.id; } }}>
          <button class="fav" class:on={item.folder.favourite} aria-label={t("Star")} onclick={(e) => starFolder(e, item.folder)}>
            <i class="ic {item.folder.favourite ? 'ic-star-fill' : 'ic-star'}"></i>
          </button>
          <div class="thumb"><i class="ic ic-folder lg"></i></div>
          <div class="title">{item.folder.name}</div>
          <div class="sub">{plural(n, 'item')}</div>
        </div>
      {:else}
        {@const c = item.card}
        {@const pick = picking && c.kind === 'part' ? (ui.partPicks ?? []).indexOf(c.id) : -1}
        <div
          class="card"
          class:selected={picking ? pick >= 0 : c.id === ui.selected}
          class:unpickable={picking && c.kind === 'job'}
          role="button"
          tabindex="0"
          aria-pressed={picking && c.kind === 'part' ? pick >= 0 : undefined}
          onclick={() => choose(c)}
          onkeydown={(e) => { if (e.key === 'Enter') choose(c); }}>
          {#if picking && c.kind === 'part'}
            <span class="pick-mark" class:on={pick >= 0} aria-hidden="true">{pick >= 0 ? pick + 1 : ''}</span>
          {/if}
          {#if c.kind === 'part' || c.kind === 'job'}
            <button class="fav" class:on={c.favourite} aria-label={t("Star")} onclick={(e) => starCard(e, c)}>
              <i class="ic {c.favourite ? 'ic-star-fill' : 'ic-star'}"></i>
            </button>
          {/if}
          {#if c.kind === 'part' && !picking}
            <button class="fav edit-sketch" title={t('Edit as sketch')} aria-label={t('Edit as sketch')}
              onclick={(e) => { e.stopPropagation(); void editPart(c.id); }}>
              <svg class="ic-svg" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20l4-1 11-11-3-3L5 16zM14 7l3 3" /></svg>
            </button>
          {/if}
          <div class="thumb">
            {#if c.image}
              <img class="part-image" src={c.image} alt={c.name} loading="lazy" />
            {:else}
            <svg viewBox={thumb(c.outline)}>
              {#each c.outline as line}
                <path d={pathOf(line)} fill="none" stroke="var(--ink)" stroke-width="2" stroke-linejoin="round" vector-effect="non-scaling-stroke"/>
              {/each}
            </svg>
            {/if}
          </div>
          <div class="title">{c.name}</div>
          <div class="sub">
            {#if everywhere && c.folder}<i class="ic ic-folder sm"></i>{`${folderTrail(c.folder, ' / ')} · `}{/if}{c.material ? `${c.material} · ` : ''}{c.size}
          </div>
          <div class="tags">
            {#if c.laser}<span class="tag {c.laser}">{laserLabel(c.laser)}</span>{/if}
            <span class="tag">{t(kindTag(c))}</span>
            {#if c.contours}<span class="tag">{plural(c.contours, 'path')}</span>{/if}
          </div>
        </div>
      {/if}
    {:else}
      <div class="empty"><h2>{emptyTitle}</h2>{emptyHint}</div>
    {/each}
  </div>

  <div class="panel-foot">
    <button class="btn btn-ghost" onclick={() => (historyOpen = true)}>{t("History")}</button>
    <span class="muted">{q ? plural(visible.length, 'result') : plural(total, 'item')}</span>
    <div class="pager">
      <button onclick={() => (page = Math.max(0, page - 1))} disabled={page === 0} aria-label={t("Previous page")}>
        <i class="ic ic-chev-left"></i>
      </button>
      <span>{page + 1} / {pages}</span>
      <button onclick={() => (page = Math.min(pages - 1, page + 1))} disabled={page >= pages - 1} aria-label={t("Next page")}>
        <i class="ic ic-chev-right"></i>
      </button>
    </div>
  </div>
{/if}
</section>
{#if editingStock}<StockEditor id={editingStock} onclose={() => editingStock = null} />{/if}

{#if kindFilter !== 'sheet'}{#if picking}<PartPicks shown={shownParts} />{:else}<PartsSide />{/if}{/if}
{#if historyOpen}<EditHistory onclose={() => (historyOpen = false)} />{/if}

<style>
  .sheet-library-page { grid-column:1 / -1; }
  :global(.card) .edit-sketch { right:56px; }
  .edit-sketch svg { width:20px; height:20px; fill:none; stroke:currentColor; stroke-width:2; stroke-linecap:round; stroke-linejoin:round; }
  .toolbar.parts-head { flex-shrink:0; justify-content:space-between; gap:12px; min-height:calc(var(--touch) + 25px); }
  .parts-actions { display:flex; align-items:center; gap:8px; min-width:0; }
  .parts-filters { flex-shrink:0; padding:12px 16px 0; gap:8px; }
  .parts-filters .search { min-width:0; flex:1 1 200px; }
  .parts-filters .seg button { padding:0 12px; }
  .parts-filters input { min-width:0; width:0; text-overflow:ellipsis; }
  .parts-head .on, .parts-filters .btn.on { border-color:var(--accent); color:var(--accent); background:var(--accent-soft); }
  .filter-menu { position:relative; }
  .filter-btn { padding:0 14px; gap:6px; }
  .filter-btn .ic-chev-down { width:16px; height:16px; }
  .filter-btn .ic-star-fill { width:18px; height:18px; }
  .filter-pop { position:absolute; top:calc(100% + 8px); left:0; z-index:20; width:280px; display:grid; gap:8px; padding:12px; background:var(--panel); border:1px solid var(--line); border-radius:var(--r); box-shadow:var(--shadow); }
  .filter-label { font-size:var(--t-sm); font-weight:600; color:var(--ink-3); }
  .fav-toggle { justify-content:flex-start; }
  .parts-grid .card { height:auto; min-height:0; }
  .part-image { width:100%; height:100%; min-height:0; object-fit:contain; }
  .parts-grid .card .thumb { flex:1 1 64px; height:auto; min-height:64px; }
  .pick-mark { position:absolute; top:8px; left:8px; width:32px; height:32px; border-radius:50%; border:2px solid var(--ink-3); background:var(--panel); display:grid; place-items:center; font-weight:700; font-size:var(--t-sm); z-index:1; }
  .pick-mark.on { background:var(--accent); border-color:var(--accent); color:#fff; }
  .card.unpickable { opacity:.45; }
</style>
