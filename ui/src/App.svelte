<script lang="ts">
  import { t } from './i18n/index';

  import Brand from './components/Brand.svelte';
  import WorkspaceOverlays from './components/WorkspaceOverlays.svelte';
  import { recipeEdits } from './lib/recipe-edits.svelte';
  import { settingsEdits } from './lib/settings-edits.svelte';
  import Parts from './routes/Parts.svelte';
  import Materials from './routes/Materials.svelte';
  import Setup from './routes/Setup.svelte';
  import Run from './routes/Run.svelte';
  import Machine from './routes/Machine.svelte';
  import Sketch from './routes/Sketch.svelte';
  import { api } from './api/client';
  import { server } from './stores/server.svelte';
  import { ui, type Tab } from './stores/ui.svelte';
  import { explain } from './lib/format';
  import { WORKFLOW, SETTINGS } from './lib/navigation';
  import { confirmDisconnect, connectionLabel } from './lib/connection';
  import { partsOf, sourceLabel } from './lib/job-parts';
  import { pending } from './lib/pending.svelte';
  import { setupAlarms } from './lib/setup-alarms';
  import { holdScrollPositions } from './lib/scroll-hold';
  import PendantChip from './components/PendantChip.svelte';
  import LaserSwitch from './components/LaserSwitch.svelte';

  // Start on the pendant: the Run page answers it as its own Start button.
  let pendantStarts: number | undefined;
  $effect(() => {
    const count = doc?.pendant?.start;
    if (count === undefined) return;
    if (pendantStarts !== undefined && count !== pendantStarts) {
      ui.tab = 'run';
      ui.pendantStart = true;
    }
    pendantStarts = count;
  });

  // A panel whose content shrinks for a moment keeps its place.
  $effect(() => holdScrollPositions());

  const doc = $derived(server.doc);
  const connected = $derived(doc?.machine.connection.state === 'connected');
  // The connect workflow's step, on the button; a click while it runs cancels.
  const link = $derived(doc?.link);
  const busy = $derived(!!link && link.phase !== 'idle' && link.phase !== 'failed');
  const label = $derived(connectionLabel(doc));
  const alarms = $derived((doc?.machine.alarms.length ?? 0) + setupAlarms(doc).length);
  const brandSub = $derived(doc?.draft ? sourceLabel(partsOf(doc.draft, doc.library.parts)) : '');

  function setTab(tab: Tab): void {
    ui.tab = tab;
  }

  function togglePage(page: Tab): void {
    ui.tab = ui.tab === page ? 'parts' : page;
  }

  async function connect(): Promise<void> {
    if (connected && !await confirmDisconnect(doc ?? null, (request) => ui.confirm(request))) return;
    try {
      await api.machine(connected ? 'disconnect' : busy ? 'cancel' : 'connect');
    } catch (error) {
      ui.say(explain(error), true);
    }
  }

  const done = $derived({
    parts: !!doc?.draft,
    setup: !!doc?.draft?.compiled,
  });
</script>

<div class="app">
  <header class="topbar">
    <div class="brand">
      <Brand />
      <span class="brand-sub">{brandSub}</span>
    </div>

    <nav class="tabs">
      {#each WORKFLOW as { id: tab, number: num, label }}
        <button class="tab" class:active={ui.tab === tab} class:done={done[tab as 'parts' | 'setup'] ?? false} onclick={() => setTab(tab as Tab)}>
          <span class="tab-num">{#if done[tab as 'parts' | 'setup']}<i class="ic ic-check"></i>{:else}{num}{/if}</span><span>{t(label)}</span>
        </button>
      {/each}
    </nav>

    <div class="topbar-actions">
      {#if pending.count > 0}<button class="btn btn-ghost page-btn pending-btn" onclick={() => (ui.modal = 'pending')}>{t("Pending changes")}<span class="badge">{pending.count}</span></button>{/if}
      <button class="btn btn-ghost icon-btn page-btn" class:active={ui.tab === 'sketch'} onclick={() => togglePage('sketch')} title={t("Sketch a part")}><svg class="ic-svg" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20l4-1 11-11-3-3L5 16zM14 7l3 3" /></svg><span>{t("Sketch")}</span></button>
      <button class="btn btn-ghost icon-btn page-btn" class:active={ui.tab === 'materials'} onclick={() => togglePage('materials')} title={t("Material library")}><i class="ic ic-layers"></i><span>{t("Materials")}</span></button>
      <button class="btn btn-ghost icon-btn page-btn" class:active={ui.tab === SETTINGS.id} onclick={() => togglePage(SETTINGS.id)} title={t(SETTINGS.label)}><i class="ic ic-gear"></i><span>{t(SETTINGS.label)}</span></button>
      <button class="btn btn-ghost icon-btn alarm" onclick={() => (ui.modal = 'alarms')} title={t("Alarms")}><i class="ic ic-bell"></i><span>{t("Alarms")}</span>{#if alarms > 0}<span class="badge">{alarms}</span>{/if}</button>
      {#if doc}<LaserSwitch />{/if}
      {#if doc?.pendant}<PendantChip pendant={doc.pendant} />{/if}
      <button class="btn btn-primary connect" class:connected class:busy onclick={connect} disabled={!server.link} title={link?.detail ?? ''}><span class="dot"></span><span>{t(label)}</span>{#if busy}<span class="cancel">{t("Cancel")}</span>{/if}</button>
    </div>
  </header>

  {#if doc?.persistence_error || recipeEdits.storageError || settingsEdits.error}<div class="persistence-error" role="alert">{doc?.persistence_error ?? recipeEdits.storageError ?? settingsEdits.error}</div>{/if}

  <main class="workspace" class:materials={ui.tab === 'materials'} class:sketch={ui.tab === 'sketch'}>
    {#if !doc}
      <section class="panel main"><div class="empty"><h2>{t("Waiting for the server")}</h2>{t("Start")} <span class="mono">openlaser</span> {t("and this page connects on its own.")}</div></section>
    {:else if ui.tab === 'parts'}
      <Parts />
    {:else if ui.tab === 'materials'}
      <Materials />
    {:else if ui.tab === 'sketch'}
      <Sketch />
    {:else if ui.tab === 'setup'}
      <Setup />
    {:else if ui.tab === 'run'}
      <Run />
    {:else}
      <Machine />
    {/if}
  </main>

  <WorkspaceOverlays />
</div>

<style>
  .pending-btn { position: relative; }
  .pending-btn .badge { margin-left: 8px; background: var(--accent); }
  .app:has(.persistence-error) { grid-template-rows:68px auto minmax(0,1fr); }
  .persistence-error { padding: 8px 20px; background: var(--warn-soft); color: var(--warn); }
  .ic-svg { width: 20px; height: 20px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; opacity: .85; }
  .workspace.sketch { grid-template-columns: minmax(0, 1fr) clamp(260px, 20vw, 320px); }
</style>
