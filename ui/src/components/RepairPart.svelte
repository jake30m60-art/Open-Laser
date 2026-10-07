<script lang="ts">
  import { t } from '../i18n/index';

  // Repairs a part's contours into a new part: gaps between ends that nearly
  // meet are closed, repeated contours go, and a contour that crosses itself
  // is split into simple loops. The part itself stays as it is for the jobs
  // that use it; `onsaved` may swap the new part into the job being set up.
  import Modal from './Modal.svelte';
  import { api } from '../api/client';
  import type { RepairView } from '../api';
  import { ui } from '../stores/ui.svelte';
  import { explain } from '../lib/format';
  import { quantity } from '../lib/units.svelte';

  let { part, onclose, onsaved }: { part: { id: string; name: string }; onclose: () => void; onsaved?: (id: string) => Promise<void> | void } = $props();
  const TOLERANCES = [[t('Fine'), 0.01], [t('Standard'), 0.05], [t('Coarse'), 0.1], [t('Rough'), 0.5]] as const;
  let tolerance = $state(0.05);
  let result = $state<RepairView | null>(null);
  let checking = $state(false), saving = $state(false), error = $state('');
  let asked = 0;

  const changed = $derived(!!result && (result.gaps > 0 || result.duplicates > 0 || result.split > 0 || result.specks > 0));
  const mm = (value: number) => quantity(value, 'mm', 3);

  // Every choice shows what it would do before anything is saved.
  $effect(() => {
    const at = tolerance, id = part.id, ask = ++asked;
    checking = true; error = '';
    api.repairPart(id, at, false)
      .then((view) => { if (ask === asked) result = view; })
      .catch((e) => { if (ask === asked) { result = null; error = explain(e); } })
      .finally(() => { if (ask === asked) checking = false; });
  });

  async function save(): Promise<void> {
    if (saving || !changed) return;
    const { id, name } = part;
    saving = true;
    try {
      const view = await api.repairPart(id, tolerance, true);
      if (view.part) await onsaved?.(view.part);
      onclose();
      ui.say(t('Saved {0} repaired', name));
    } catch (e) {
      error = explain(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={t('Repair contours')} onclose={() => { if (!saving) onclose(); }}>
  <p class="muted">{t('Closes gaps where ends nearly meet, drops contours that repeat another, and splits a contour that crosses itself into simple loops. A loop smaller than the tolerance is a tracing glitch and is dropped. Nothing moves further than the tolerance. It is saved as a new part; {0} stays as it is for the jobs that use it.', part.name)}</p>
  <div class="tolerances" role="group" aria-label={t('Tolerance')}>
    {#each TOLERANCES as [label, value]}
      <button aria-pressed={tolerance === value} disabled={saving} onclick={() => (tolerance = value)}><strong>{t(label)}</strong><small>{mm(value)}</small></button>
    {/each}
  </div>
  <dl class="outcome" aria-live="polite">
    {#if result}
      <dt>{t('Faulty contours')}</dt>
      <dd class:bad={result.faulty.length > 0}>{result.faulty.length} → <strong class:bad={result.unresolved.length > 0}>{result.unresolved.length}</strong></dd>
      <dt>{t('Contours')}</dt><dd>{result.contours_before} → <strong>{result.contours_after}</strong></dd>
      {#if result.gaps}<dt>{t('Gaps closed')}</dt><dd>{result.gaps}</dd>{/if}
      {#if result.split}<dt>{t('Split into loops')}</dt><dd>{result.split}</dd>{/if}
      {#if result.specks}<dt>{t('Tiny loops removed')}</dt><dd>{result.specks}</dd>{/if}
      {#if result.duplicates}<dt>{t('Repeated contours removed')}</dt><dd>{result.duplicates}</dd>{/if}
    {:else if checking}
      <dt>{t('Checking…')}</dt><dd></dd>
    {/if}
  </dl>
  {#if result && !changed}<p class="gate-reason">{t('Nothing to repair at this tolerance.')}</p>{/if}
  {#if result && result.unresolved.length}<p class="error" role="alert">{t('{0} contours still cross themselves or touch another and need editing in the sketch.', result.unresolved.length)}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="actions">
    <button class="btn btn-ghost lg" disabled={saving} onclick={onclose}>{t('Cancel')}</button>
    <button class="btn btn-primary lg" disabled={saving || checking || !changed} onclick={save}>{saving ? t('Saving…') : t('Save as new part')}</button>
  </div>
</Modal>

<style>
  .muted { margin: 0 0 16px; line-height: 1.6; }
  .tolerances { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; }
  .tolerances button { min-height: 64px; display: grid; gap: 4px; place-content: center; border: 1px solid var(--line); border-radius: 10px; background: var(--panel-2); color: var(--ink); cursor: pointer; }
  .tolerances button[aria-pressed="true"] { border-color: var(--accent); color: var(--accent); background: var(--accent-soft); }
  .tolerances small { color: var(--ink-3); font-size: var(--t-sm); }
  .outcome { display: grid; grid-template-columns: auto 1fr; gap: 10px 16px; margin: 20px 0 8px; min-height: 52px; }
  .outcome dt { color: var(--ink-3); }
  .outcome dd { margin: 0; display: flex; flex-wrap: wrap; gap: 4px 10px; align-items: baseline; }
  .bad { color: var(--danger); }
  .error { color: var(--warn); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
</style>
