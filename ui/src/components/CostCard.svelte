<script lang="ts">
  import { t } from '../i18n/index';

  // Costs of the job being set up: raw material by whole sheets (the rest
  // of the last sheet credited), gas, machine time, handling, setup and
  // markup, with the project total and the price per part. Raw material
  // prices live in the recipe, shop rates under Settings → Gas costs.
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { SETTINGS_PAGES } from '../lib/navigation';
  import { explain } from '../lib/format';
  import { numberAtLeast } from '../lib/prompt';
  import { materialKey } from '../lib/materials';
  import { duration, money } from '../lib/gas';
  import { PRICE_PER_KG, PRICE_PER_M2, density, estimate, materialPrice, pricePerM2 } from '../lib/costing';

  const doc = $derived(server.doc!);
  const draft = $derived(doc.draft!);
  const costs = $derived(doc.gas.costs);
  const currency = $derived(costs.currency);
  // Prices are read from the library, so a price entered later counts even
  // where the job keeps an older copy of its recipe.
  const recipe = $derived(draft.recipe ? (doc.library.recipes.find((r) => r.id === draft.recipe!.id) ?? draft.recipe) : null);
  const price = $derived(recipe ? materialPrice(recipe.name, recipe.attributes) : null);
  let open = $state(false);

  // Without a sheet the parts' bounding rectangle stands in for one.
  const bounds = $derived(draft.preview?.bounds ?? null);
  const noSheet = $derived(draft.stock_outline.length < 3);
  const sheet = $derived<[number, number][]>(!noSheet ? draft.stock_outline
    : bounds ? [[bounds.min.x, bounds.min.y], [bounds.max.x, bounds.min.y], [bounds.max.x, bounds.max.y], [bounds.min.x, bounds.max.y]] : []);
  const result = $derived(draft.compiled && sheet.length >= 3 ? estimate({
    rates: costs.shop,
    price,
    thickness: recipe?.thickness_mm ?? 0,
    sheet,
    cutouts: noSheet ? [] : draft.stock_cutouts,
    sheets: draft.sheets?.pages.length ?? 1,
    partsPerSheet: draft.sheets ? draft.sheets.pages.map((p, i) => (i === draft.sheets!.active ? draft.groups.length : p.parts ?? null)) : [draft.groups.length],
    active: draft.sheets?.active ?? 0,
    seconds: draft.compiled.seconds,
    gas: doc.gas.estimate?.cost ?? null,
    used: !noSheet && draft.preview?.bounds ? { minX: draft.preview.bounds.min.x, minY: draft.preview.bounds.min.y, maxX: draft.preview.bounds.max.x, maxY: draft.preview.bounds.max.y } : null,
  }, {
    sheets: (n, m2) => t('{0} × {1} m² sheet', n, m2.toFixed(2)) + (price && recipe ? ` · ${money(pricePerM2(price, recipe.thickness_mm), currency)}/m²` : ''),
    remnant: (m2) => t('{0} m² back to stock', m2.toFixed(2)),
    hours: (s, rate) => `${duration(s)} × ${money(rate, currency)}/h`,
    handling: (n, min) => t('{0} sheets × {1} min', n, min),
    setup: () => t('Once per job'),
    gas: () => (doc.gas.estimate?.cost === null ? t('No gas price yet') : t('From gas costs')),
  }) : null);

  const NAMES = { material: 'Raw material', remnant: 'Remnant credit', gas: 'Gas', machine: 'Machine time', handling: 'Sheet handling', setup: 'Setup fee' } as const;
  const brief = $derived(result ? `${money(result.total, currency)} · ${result.perPart === null ? '—' : t('{0} per part', money(result.perPart, currency))}` : t('After compiling'));

  const settingsPage = SETTINGS_PAGES.find(p => p.groups.includes('gas'))?.id;
  function openSettings(): void {
    if (settingsPage === undefined) return;
    ui.machinePage = settingsPage;
    ui.tab = 'machine';
  }
  /** The raw material price per kg, for every thickness of this material. */
  function editPrice(): void {
    if (!recipe) return;
    const r = recipe;
    if (!density(r.name, r.attributes)) { ui.say(t('Set a price per m² for this material in the recipe.'), true); return; }
    numberAtLeast(t('Raw material price per kg'), Number(r.attributes[PRICE_PER_KG]) || 0, `${currency}/kg`, (v) => {
      const members = doc.library.recipes.filter((m) => materialKey(m) === materialKey(r));
      Promise.all(members.map((m) => api.updateRecipe(m.id, { attributes: { [PRICE_PER_KG]: String(v) } })))
        .catch((e) => ui.say(explain(e), true));
    });
  }
  const perM2 = $derived(recipe && Number(recipe.attributes[PRICE_PER_M2]) > 0);
</script>

<div class="card2 cost-card" class:folded={!open}>
  <button class="fold" aria-expanded={open} onclick={() => (open = !open)}>
    <span><strong>{t('Costs')}</strong><small>{brief}</small></span>
    <i class="ic ic-chev-down" class:open></i>
  </button>
  {#if open}
    {#if result}
      <div class="stats">
        <div><b>{money(result.total, currency)}</b><span>{t('Project total')}</span></div>
        <div><b>{money(result.perPart, currency)}</b><span>{t('Per part ({0})', result.parts)}</span></div>
        <div><b>{draft.sheets?.pages.length ?? 1}</b><span>{t('Sheets')}</span></div>
      </div>
      <div class="table" role="table" aria-label={t('Cost breakdown')}>
        {#each result.lines as line (line.key)}
          <div class="row" role="row" class:missing={line.amount === null}>
            <span role="cell"><b>{t(NAMES[line.key])}</b><small>{line.detail}</small></span>
            <span role="cell">{line.amount === null ? '—' : money(line.amount, currency)}</span>
          </div>
        {/each}
        <div class="row sum" role="row"><span role="cell"><b>{t('Net')}</b></span><span role="cell">{money(result.net, currency)}</span></div>
        {#if costs.shop.margin_percent > 0}
          <div class="row" role="row"><span role="cell"><b>{t('Markup {0} %', costs.shop.margin_percent)}</b></span><span role="cell">{money(result.margin, currency)}</span></div>
        {/if}
        <div class="row sum total" role="row"><span role="cell"><b>{t('Total')}</b></span><span role="cell">{money(result.total, currency)}</span></div>
      </div>
      {#if noSheet}<p class="note">{t('No sheet chosen: the material is the parts’ bounding rectangle.')}</p>{/if}
      {#if result.scaled}<p class="note">{t('Other sheets are estimated from this sheet’s time per part; open each sheet for its own time.')}</p>{/if}
      {#if result.missing.includes('material')}
        <p class="note">{t('No raw material price in the recipe yet.')} <button class="link-btn" onclick={editPrice}>{t('Enter price per kg')}</button></p>
      {:else if !perM2}
        <p class="note">{t('Material {0}/kg', money(Number(recipe?.attributes[PRICE_PER_KG]), currency))} · {t('{0} g/cm³', density(recipe!.name, recipe!.attributes))} <button class="link-btn" onclick={editPrice}>{t('Change')}</button></p>
      {/if}
      {#if settingsPage !== undefined}<p class="note"><button class="link-btn" onclick={openSettings}>{t('Hourly rate, setup and markup')}</button></p>{/if}
    {:else}
      <p class="note">{t('Costs appear once the job is compiled.')}</p>
    {/if}
  {/if}
</div>

<style>
  .cost-card.folded { padding-top:4px; padding-bottom:4px; }
  .fold { display:flex; align-items:center; justify-content:space-between; gap:10px; min-height:52px; padding:0; border:0; background:transparent; color:var(--ink); font:inherit; text-align:left; cursor:pointer; }
  .fold > span { display:grid; gap:2px; min-width:0; }
  .fold strong { font-size:var(--t-base); }
  .fold small { font-size:var(--t-sm); color:var(--ink-3); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .fold .ic { flex:none; transition:transform .15s; }
  .fold .ic.open { transform:rotate(180deg); }
  .cost-card .stats span { font-size: var(--t-sm); }
  .note { margin: 0; font-size: var(--t-sm); color: var(--ink-3); }
  .link-btn { min-height: 44px; padding: 0 6px; border: 0; background: none; color: var(--accent-2); font-weight: 600; font-size: var(--t-sm); cursor: pointer; }
  .table { display: grid; gap: 2px; }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 8px; align-items: center; min-height: 40px; padding: 4px 8px; border-radius: 8px; font-size: var(--t-sm); font-variant-numeric: tabular-nums; }
  .row:nth-child(even) { background: var(--panel-2); }
  .row > span:first-child { display: grid; min-width: 0; }
  .row small { color: var(--ink-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row > span:last-child { text-align: right; white-space: nowrap; }
  .row.missing > span:last-child { color: var(--ink-3); }
  .row.sum { border-top: 1px solid var(--line); border-radius: 0; }
  .row.total { font-size: var(--t-base); font-weight: 700; }
</style>
