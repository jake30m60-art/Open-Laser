<script lang="ts">
  import { materialLabel } from '../../lib/materials';
  import { t as tr } from '../../i18n/index';

  import { inputValue, quantity, sourceInput, unitLabel, units } from '../../lib/units.svelte';
  // The selected recipe: its picture, identity and touch actions on top,
  // with the setup note under them, then Cutting,
  // Piercing and Process options. Tapping a number opens the keypad, a
  // flag flips in place, a gas is its chips, and text opens the keyboard.
  // Everything stays staged until Save.
  import Cutting from './Cutting.svelte';
  import { airRecipeUsesNitrogen } from '../../lib/recipe-source';
  import Piercing from './Piercing.svelte';
  import Options from './Options.svelte';
  import Engraving from './Engraving.svelte';
  import { PROCESS, engravingAttributes } from '../../lib/engraving';
  import Sheets from './Sheets.svelte';
  import MaterialSummary from '../../components/MaterialSummary.svelte';
  import { summaryOf, type SummaryKey } from '../../lib/summary';
  import { api } from '../../api/client';
  import { server } from '../../stores/server.svelte';
  import { ui } from '../../stores/ui.svelte';
  import { osk } from '../../lib/osk.svelte';
  import { ago, explain, laserLabel, plural } from '../../lib/format';
  import { pictureOf, materialKey } from '../../lib/materials';
  import { available, field, num, numeric, refused, toggled, type Editor, type Values } from '../../lib/recipe';
  import type { RecipeView } from '../../api';

  let { recipe, values, edits, film, onset, onfilm, oncopy, onsave, ondiscard, saving }: {
    recipe: RecipeView;
    saving: boolean;
    /** The file's values with the staged edits over them. */
    values: Values;
    edits: Values;
    /** The film process as staged. */
    film: RecipeView | null;
    onset: (key: string, value: string) => void;
    onfilm: (id: string | null) => void;
    oncopy: () => void;
    onsave: () => void;
    ondiscard: () => unknown;
  } = $props();

  const doc = $derived(server.doc!);
  let page = $state<'cutting' | 'piercing' | 'options' | 'engraving'>('cutting');
  /** An engraving recipe, as staged. */
  const engraving = $derived(values[PROCESS] === 'engrave');
  let openedRecipe: string | null = null;
  $effect(() => {
    if (openedRecipe !== recipe.id) {
      openedRecipe = recipe.id;
      page = engraving ? 'engraving' : 'cutting';
    }
  });
  $effect(() => { if (!engraving && page === 'engraving') page = 'cutting'; });
  /** Marks the recipe for engraving, with a fill to start from, or back for cutting. */
  function toggleProcess(): void {
    if (engraving) { onset(PROCESS, 'cut'); return; }
    for (const [key, value] of Object.entries(engravingAttributes())) if (key === PROCESS || values[key] === undefined) onset(key, value);
    page = 'engraving';
  }
  let sheet = $state<string | null>(null);
  /** The visible piercing stage, counted from the first that runs. */
  let visible = $state(0);
  /** The key the keypad is typing into. */
  let editing = $state<string | null>(null);
  $effect(() => { if (!osk.open) editing = null; });
  const pending = $derived(Object.keys(edits).length + ((film?.id ?? null) === (recipe.film ?? null) ? 0 : 1));
  const a = $derived(available(doc.files.capabilities, recipe.laser));
  /** The summary as the recipe stands with its staged edits. */
  const summary = $derived.by(() => { const s = summaryOf(values, recipe.laser); return 'CutGasType' in edits ? s : { ...s, gas: recipe.gas }; });

  /** Opens the control for a key: a flip, the keyboard, or the keypad with the field's limits. */
  function tap(key: string, override?: { value: string; commit: (value: string) => void }): void {
    const f = field(key);
    const current = override?.value ?? values[key] ?? '';
    const system = units.system;
    const commit = override?.commit ?? ((t: string) => onset(key, t));
    if (f.kind === 'flag') { commit(toggled(current)); return; }
    if (f.kind === 'text' && !numeric(current)) { osk.text(f.label, current, commit); return; }
    const ask = (start: string) => {
      editing = key;
      osk.show({
        kind: 'num', label: f.label, value: start, unit: unitLabel(f.unit ?? '', system), fresh: true,
        onCommit: (t) => {
          const n = sourceInput(t, Number(current), f.unit ?? '', system);
          const why = t.trim() === '' ? tr("Enter a number.") : refused(key, n);
          if (why) { ui.say(`${f.label}: ${why}`, true); ask(t); return; }
          commit(n === Number(current) ? current : num(n));
        },
      });
    };
    ask(numeric(current) ? inputValue(Number(current), f.unit ?? '', system) : '');
  }
  /** A summary tile opens its value: the field on its page, or the setup. */
  function editSummary(key: SummaryKey): void {
    const fields: Partial<Record<SummaryKey, string>> = {
      speed: 'CutSpeed', power: 'CutPeakCurrent', duty: recipe.laser === 'co2' ? 'CutDuty' : 'CutPower',
      frequency: 'CutFreq', height: 'CutHeight',
    };
    if (key === 'pierce') { page = 'piercing'; return; }
    if (key === 'nozzle' || key === 'focus' || key === 'lens') { note(); return; }
    page = 'cutting';
    const field = fields[key];
    if (field) tap(field);
  }
  const ed = $derived<Editor>({ values, edits, editing, laser: recipe.laser, a, set: onset, setMany: (many) => { for (const [key, value] of Object.entries(many)) onset(key, value); }, tap });

  // The rare actions: the material's name and star cover every recipe of it.
  const art = $derived(pictureOf(recipe.name, recipe.photo));
  const noteLine = $derived(recipe.note.split(/\s*\n+\s*/)
    .filter(line => line
      && !(values['OpenLaserNozzleDiameter'] && line.startsWith('NOZZLE:'))
      && !(values['OpenLaserManualFocus'] !== undefined && line.startsWith('FOCUS (SOURCE NOTE/FILENAME):')))
    .map((line) => line.replace(/\s+/g, ' ')).join(' · '));
  const members = () => doc.library.recipes.filter((r) => materialKey(r) === materialKey(recipe));
  async function run(action: () => Promise<unknown>, then?: () => void): Promise<void> {
    try { await action(); then?.(); } catch (error) { ui.say(explain(error), true); }
  }
  const rename = () => osk.text(tr("Material"), recipe.name, (name) => { if (name.trim()) run(() => Promise.all(members().map((r) => api.updateRecipe(r.id, { name: name.trim(), attributes: {} }))), () => (ui.selectedRecipe = recipe.id)); });
  const thickness = () => osk.number(tr("Thickness"), recipe.thickness_mm, 'mm', (v) => { if (v >= 0) run(() => api.updateRecipe(recipe.id, { thickness_mm: v, attributes: {} })); });
  const note = () => osk.text(tr("Setup note"), recipe.note, (text) => run(() => api.updateRecipe(recipe.id, { note: text, attributes: {} })));
  const star = () => run(() => Promise.all(members().map((r) => api.updateRecipe(r.id, { favourite: !recipe.favourite, attributes: {} }))));
  const remove = async () => {
    if (pending || saving) { ui.say(tr("Save or discard this recipe’s edits before deleting it."), true); return; }
    const confirmed = await ui.confirm({
      title: tr("Delete this recipe?"),
      body: tr("{0} leaves the material library. Saved jobs keep their own copy, and library history can undo it.", recipe.name),
      confirm: tr("Delete recipe"),
      danger: true,
    });
    if (confirmed) run(() => api.removeRecipe(recipe.id), () => ui.say(tr("Recipe removed; saved jobs keep their copy.")));
  };
  const use = () => run(() => api.setRecipe(recipe.id), () => (ui.tab = 'setup'));
</script>

<section class="panel main">
  <div class="panel-head recipe-head">
    <div class="recipe-identity">
      {#if art}<img class="art" src={art} alt={recipe.photo ? tr('Sample cut') : ''}>{:else}<div class="art blank"><i class="ic ic-layers"></i></div>{/if}
      <div class="recipe-details">
        <div class="recipe-eyebrow"><h3>{tr("Material library")}</h3><span class="tag {recipe.laser}">{laserLabel(recipe.laser)}</span>{#if engraving}<span class="tag">{tr("Engraving")}</span>{/if}</div>
        <h1>{materialLabel(recipe.name)}</h1>
        {#if !engraving}<div class="recipe-spec"><strong>{recipe.thickness_mm > 0 ? quantity(recipe.thickness_mm, 'mm') : tr("No thickness set")}</strong><span>{tr("sheet thickness")}</span></div>{/if}
        <div class="recipe-source"><span title={recipe.file_name ?? tr('Bank {0}', recipe.layer)}>{recipe.file_name ?? tr('Bank {0}', recipe.layer)}</span>{#if recipe.updated}<span>{tr("Edited {0}", ago(recipe.updated))}</span>{/if}</div>
      </div>
    </div>
    <div class="recipe-use">
      <button
        class="btn btn-ghost icon-only star" class:on={recipe.favourite} onclick={star}
        aria-label={recipe.favourite ? tr('Unstar material') : tr('Star material')} aria-pressed={recipe.favourite}
      ><i class="ic {recipe.favourite ? 'ic-star-fill' : 'ic-star'}"></i></button>
      <button class="btn btn-primary" onclick={use} disabled={!doc.draft}>{doc.draft ? tr("Use in current job") : tr("Open a part to use it")}</button>
    </div>
    <div class="recipe-actions" role="group" aria-label={tr("Recipe actions")}>
      <button class="btn btn-ghost" onclick={rename}>{tr("Rename")}</button>
      {#if !engraving}<button class="btn btn-ghost" data-numpad onclick={thickness}>{tr("Thickness")}</button>{/if}
      <button class="btn btn-ghost" onclick={() => (sheet = 'inspector')}>{tr("Imported values")}</button>
      {#if !engraving}<button class="btn btn-ghost" onclick={oncopy}><i class="ic ic-copy"></i>{tr("Copy to thickness")}</button>{/if}
      <button class="btn btn-ghost" onclick={toggleProcess}>{engraving ? tr("Make it a cutting recipe") : tr("Make it an engraving recipe")}</button>
      <button class="btn btn-ghost recipe-delete" onclick={remove}><i class="ic ic-trash"></i>{tr("Delete recipe")}</button>
    </div>
  </div>
  <div class="setup-strip">
    <span><b>{tr("Setup")}</b><span class="text">{noteLine || tr('No setup note yet')}</span></span>
    <button class="btn btn-ghost" onclick={note}>{tr("Edit setup")}</button>
  </div>
  {#if airRecipeUsesNitrogen(recipe, values)}
    <p class="gas-source-warning" role="alert">{tr("Air recipe on the N₂ valve, as on the machine's own fiber layers: it cuts with whatever is connected to the N₂ inlet. Check the supply before cutting.")}</p>
  {/if}
  <div class="recipe-summary"><MaterialSummary source={summary} ontap={editSummary} /></div>
  <div class="recipe-tabs">
    <div class="seg">
      <button class:on={page === 'cutting'} onclick={() => (page = 'cutting')}>{tr("Cutting")}</button>
      <button class:on={page === 'piercing'} onclick={() => (page = 'piercing')}>{tr("Piercing")}</button>
      <button class:on={page === 'options'} onclick={() => (page = 'options')}>{tr("Process options")}</button>
      {#if engraving}<button class:on={page === 'engraving'} onclick={() => (page = 'engraving')}>{tr("Engraving")}</button>{/if}
    </div>
    <span class="muted">{tr("Tap a value to edit it")}</span>
  </div>
  <div class="recipe-body">
    {#if page === 'cutting'}
      <Cutting {ed} onpiercing={() => (page = 'piercing')} />
    {:else if page === 'piercing'}
      <Piercing {ed} {visible} onvisible={(i) => (visible = i)} onmode={() => (sheet = 'mode')} onrefine={() => (sheet = 'refine')} />
    {:else if page === 'engraving'}
      <Engraving {ed} />
    {:else}
      <Options {ed} {film} onopen={(id) => (sheet = id)} />
    {/if}
  </div>
  <div class="panel-foot">
    <span class="changes" class:some={pending}>{pending ? plural(pending, 'unsaved change') : tr("No unsaved changes")}</span>
    <div class="actions">
      <button class="link" onclick={() => (sheet = 'review')} disabled={!pending}>{tr("Review changes")}</button>
      <button class="btn btn-ghost" onclick={ondiscard} disabled={!pending || saving}>{tr("Discard")}</button>
      <button class="btn btn-primary" onclick={onsave} disabled={!pending || saving}><i class="ic ic-check"></i>{saving ? tr("Saving…") : tr("Save recipe")}</button>
    </div>
  </div>
</section>

{#if sheet}
  <Sheets {ed} which={sheet} {recipe} {visible} recipes={doc.library.recipes} {film} {onfilm} onclose={() => (sheet = null)} />
{/if}
<style>.gas-source-warning { padding:12px; margin:8px 16px; border:1px solid var(--warn); border-radius:8px; color:var(--ink); }</style>
