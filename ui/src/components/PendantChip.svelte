<script lang="ts">
  // The pendant at a glance in the top bar: whether its receiver is in,
  // how the arrows move, and the key held down right now.
  import { t } from '../i18n/index';
  import type { PendantView } from '../api';

  let { pendant }: { pendant: PendantView } = $props();

  const MODES: Record<string, string> = { slow: 'Slow', fast: 'Fast', step: 'Step 1 mm' };
  const KEYS: Record<string, string> = {
    Follow: 'Follow', Shutter: 'Shutter', RedLight: 'Red light', Laser: 'Laser', Blow: 'Blow',
    Frame: 'Frame', GoOrigin: 'Go to origin', Start: 'Start', Pause: 'Pause', Stop: 'Stop',
    Fn: 'Fn', ZCalibrate: 'Z calibration', FastStep: 'Fast/Step', ZDown: 'Z−', ZUp: 'Z+',
    Up: 'Y+', Down: 'Y−', Left: 'X−', Right: 'X+',
  };
  const key = $derived(pendant.key ? `${pendant.with_fn ? 'Fn+ ' : ''}${t(KEYS[pendant.key] ?? pendant.key)}` : null);
</script>

<div class="pendant" class:off={!pendant.connected} class:pressed={!!key} role="status" aria-live="polite"
  title={pendant.connected ? t("Pendant connected") : t("Pendant receiver not found")}>
  <svg class="ic-svg" viewBox="0 0 24 24" aria-hidden="true"><rect x="7" y="2" width="10" height="20" rx="3" /><circle cx="12" cy="8" r="2" /><path d="M10 14h4M10 17h4" /></svg>
  {#if !pendant.connected}
    <span>{t("Pendant off")}</span>
  {:else}
    <span class="mode">{t(MODES[pendant.mode] ?? pendant.mode)}</span>
    {#if key}<span class="key">{key}</span>{/if}
  {/if}
</div>

<style>
  .pendant { display:flex; align-items:center; gap:6px; min-height:44px; padding:0 10px; border-radius:10px;
    border:1px solid var(--line); color:var(--ink-2); font-size:var(--t-sm); white-space:nowrap; }
  .pendant .ic-svg { width:18px; height:18px; fill:none; stroke:currentColor; stroke-width:1.6; }
  .pendant .mode { font-weight:600; color:var(--ink); }
  .pendant .key { padding:2px 8px; border-radius:6px; background:var(--accent); color:var(--accent-ink); font-weight:600; }
  .pendant.pressed { border-color:var(--accent); }
  .pendant.off { opacity:.6; }
</style>
