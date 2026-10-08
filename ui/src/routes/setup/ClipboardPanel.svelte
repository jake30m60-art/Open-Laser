<script lang="ts">
  import { t } from '../../i18n/index';

  import { displayNumber, quantity, unitLabel } from '../../lib/units.svelte';
  import { osk } from '../../lib/osk.svelte';
  import { plural } from '../../lib/format';
  import type { CopiedShapes, PasteDirection, PasteSettings } from '../../lib/copy-paste';
  import { ui } from '../../stores/ui.svelte';

  let { clipboard, settings = $bindable(), disabled, pasting, onpaste }: {
    clipboard: CopiedShapes | null; settings: PasteSettings; disabled: boolean; pasting: boolean; onpaste: () => void;
  } = $props();
  const directions: { value: PasteDirection; label: string }[] = [
    { value: 'left', label: t("← Left") }, { value: 'right', label: t('Right →') },
    { value: 'up', label: t("↑ Up") }, { value: 'down', label: t("↓ Down") },
  ];
  function copies(): void {
    osk.number('Number of copies', settings.count, 'copies', value => {
      if (Number.isInteger(value) && value >= 1 && value <= 500) settings.count = value;
      else ui.say('Enter a whole copy count from 1 to 500.', true);
    });
  }
  function gap(): void {
    osk.number('Gap between copies', settings.gap, 'mm', value => {
      if (value >= 0) settings.gap = value;
      else ui.say(t('Use a gap of {0} or more.', quantity(0, 'mm')), true);
    });
  }
</script>

<div class="feat-head"><h2>{t("Copy / paste")}</h2><button class="btn btn-ghost" onclick={() => (ui.setupPanel = null)}>{t("Close")}</button></div>
{#if clipboard}
  <p class="feat-desc">{t('Each copy contains {0} · {1}.', plural(clipboard.grouping.length, 'group'), plural(clipboard.contours.length, 'contour'))}</p>
  <fieldset disabled={pasting}>
    <div class="copy-row"><span>{t("Number of copies")}</span><button class="num" aria-label={t("Number of copies")} onclick={copies}>{settings.count}</button></div>
    <div class="copy-row"><span>{t("Gap between copies")}</span><button class="num" aria-label={t("Gap between copies")} onclick={gap}>{displayNumber(settings.gap, 'mm')} <small>{unitLabel('mm')}</small></button></div>
    <p class="direction-label" id="paste-direction">{t("Paste direction")}</p>
    <div class="directions" role="group" aria-labelledby="paste-direction">{#each directions as direction}<button
      class="choice" aria-pressed={settings.direction === direction.value} onclick={() => (settings.direction = direction.value)}
    >{direction.label}</button>{/each}</div>
  </fieldset>
  <button class="btn btn-primary lg block paste" disabled={disabled || pasting} onclick={onpaste}>{pasting ? t('Pasting…') : `Paste ${plural(settings.count, 'copy', 'copies')}`}</button>
  <p class="muted copy-note">{t("Each paste continues from the last copy. Undo removes the whole batch.")}</p>
{:else}
  <p class="muted">{t("Select shapes on the drawing, then tap Copy.")}</p>
{/if}

<style>
  fieldset { border: 0; padding: 0; margin: 0; min-width: 0; }
  .copy-row { display: flex; justify-content: space-between; align-items: center; gap: 12px; min-height: 58px; font-size: var(--t-sm); }
  .num { min-width: 95px; padding: 12px; border: 1px solid var(--line); border-radius: 9px; background: var(--panel-2); color: var(--ink); font: inherit; text-align: right; }
  .num small { color: var(--ink-3); }
  .directions { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
  .choice { min-height: 44px; border: 1px solid var(--line); border-radius: 9px; background: var(--panel-2); color: var(--ink-2); font: inherit; font-size: var(--t-sm); cursor: pointer; }
  .choice[aria-pressed="true"] { border-color: var(--accent); background: var(--accent-soft); color: var(--accent); }
  .direction-label { font-size: var(--t-sm); margin: 12px 0 8px; }
  .paste { margin-top: 24px; }
  .copy-note { font-size: var(--t-sm); line-height: 1.6; margin-top: 12px; }
</style>
