<script lang="ts">
  import { t } from '../../i18n/index';

  // One choice from a row of chips.
  let { label, options, current, disabled = false, apply }: {
    label: string;
    /** Each option's value and its words. */
    options: ReadonlyArray<readonly [string, string]>;
    current: string;
    disabled?: boolean;
    apply: (v: string) => void;
  } = $props();
</script>

<div class="field">
  <div class="lbl" style="font-weight:600;font-size:var(--t-base)">{t(label)}</div>
  <div class="chips">
    {#each options as [value, text]}
      <button class="chip" class:on={current === value} onclick={() => apply(value)} {disabled}>{t(text)}</button>
    {/each}
  </div>
</div>
