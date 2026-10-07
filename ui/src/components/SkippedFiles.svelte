<script lang="ts">
  import { t } from '../i18n/index';
  // Library files the server could not load stay on disk untouched; say so
  // until they are fixed, rather than letting parts or jobs quietly vanish.
  import { server } from '../stores/server.svelte';

  const skipped = $derived(server.doc?.library.skipped ?? []);
</script>

{#if skipped.length}
  <div class="skipped-files" role="alert">
    <details>
      <summary>{t(skipped.length === 1 ? '{0} library file was not loaded' : '{0} library files were not loaded', skipped.length)}</summary>
      <p>{t("Each is left untouched in the data folder. Fix or remove it, then restart OpenLaser.")}</p>
      <ul>{#each skipped as file (file.file)}<li><code>{file.file}</code> · {file.reason}</li>{/each}</ul>
    </details>
  </div>
{/if}
