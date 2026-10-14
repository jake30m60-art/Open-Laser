<script lang="ts">
  // Fiber or CO₂ in the top bar, as Mlaser's laser choice: held to switch,
  // since it moves the heads. The jobs and recipes offered follow it; the
  // material library keeps showing both.
  import { t } from '../i18n/index';
  import HoldButton from './HoldButton.svelte';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { access } from '../lib/access.svelte';
  import { explain, laserLabel } from '../lib/format';
  import { plain } from '../lib/plain';

  const MODES = ['fiber', 'co2'] as const;
  const doc = $derived(server.doc!);
  const gate = $derived(doc.readiness.mode);
  const reason = $derived(!gate.ok && gate.reason ? plain(gate.reason).text : t("Hold to switch · the XY reference is kept"));

  async function choose(mode: 'fiber' | 'co2'): Promise<void> {
    try {
      await api.machine('mode', { mode });
      ui.say(t('Laser: {0}', laserLabel(mode)));
    } catch (error) { ui.say(explain(error), true); }
  }
</script>

<div class="laser-switch" role="group" aria-label={t("Laser mode")} title={reason}>
  {#each MODES as mode}
    {#if doc.mode === mode}
      <button class="on" aria-pressed="true" disabled>{laserLabel(mode)}</button>
    {:else}
      <HoldButton class="" disabled={!gate.ok || !access.canControl} title={reason} onhold={() => choose(mode)}>{laserLabel(mode)}</HoldButton>
    {/if}
  {/each}
</div>

<style>
  .laser-switch { display:inline-flex; gap:2px; padding:3px; border:1px solid var(--line); border-radius:12px; background:var(--panel-2); }
  .laser-switch :global(button) { min-height:38px; min-width:64px; padding:0 12px; border:0; border-radius:9px; background:transparent;
    color:var(--ink-2); font:inherit; font-weight:600; font-size:var(--t-sm); cursor:pointer; white-space:nowrap; }
  .laser-switch :global(button.on) { background:var(--accent); color:var(--accent-ink); opacity:1; cursor:default; }
  .laser-switch :global(button:disabled:not(.on)) { opacity:.45; cursor:not-allowed; }
</style>
