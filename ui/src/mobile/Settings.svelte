<script lang="ts">
  import { t } from '../i18n/index';

  import { ui } from '../stores/ui.svelte';
  import { SETTINGS_PAGES } from '../lib/navigation';
  import { APP_VERSION } from '../lib/version';
  import { pending } from '../lib/pending.svelte';
  import Machine from '../routes/Machine.svelte';
  let { open, materials }: { open:(page:number) => void; materials:() => void } = $props();
</script>

<div class="phone-page-title"><h1>{t("Settings")}</h1><span class="version">v{APP_VERSION}</span></div>
<label class="search phone-settings-search"><i class="ic ic-search"></i><input type="search" aria-label={t("Search settings")} placeholder={t("Search settings")} bind:value={ui.settingsQuery} /></label>
{#if ui.settingsQuery.trim()}<Machine compact sectionOnly />{:else}
<div class="phone-action-list phone-settings-list">{#each SETTINGS_PAGES as page}<button onclick={() => open(page.id)}>{t(page.label)}<i class="ic ic-arrow-right"></i></button>{/each}</div>
<div class="phone-action-list phone-settings-list">
  <button onclick={materials}>{t("Material library")}<i class="ic ic-arrow-right"></i></button>
  {#if pending.count > 0}<button onclick={() => ui.modal = 'pending'}>{t('Pending changes')} · {pending.count}<i class="ic ic-arrow-right"></i></button>{/if}
</div>
{/if}

<style>.version { color: var(--ink-3); font-size: var(--t-sm); } .phone-settings-search { flex: none; min-width: 0; margin-bottom: 12px; }</style>
