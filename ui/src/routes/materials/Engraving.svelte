<script lang="ts">
  import { t as tr } from '../../i18n/index';
  import Field from './Field.svelte';
  import { fillOf } from '../../lib/engraving';
  import FillSample from '../../components/FillSample.svelte';
  import type { Editor } from '../../lib/recipe';
  let { ed }: { ed: Editor } = $props();
  const fill = $derived(fillOf(ed.values));
  const fixed = $derived((ed.values['OpenLaserEngravingHeight'] ?? (ed.laser === 'co2' ? 'fixed' : 'follow')) === 'fixed');
</script>

<div class="section-head"><h2>{tr("Engraving: laser and speed")}</h2></div>
<div class="fields big">
  <Field {ed} key="CutSpeed" label="Engraving speed" big />
  {#if ed.a.peak}<Field {ed} key="CutPeakCurrent" big />{/if}
  <Field {ed} key={ed.laser === 'fiber' ? 'CutPower' : 'CutDuty'} label="Duty Cycle" big />
  <Field {ed} key="CutFreq" label="Frequency" big />
</div>
{#if ed.a.height}
  <div class="section-head"><h2>{tr("Engraving height")}</h2></div>
  <div class="seg" role="group" aria-label={tr("Engraving height mode")}>
    <button class:on={!fixed} aria-pressed={!fixed} onclick={() => ed.set('OpenLaserEngravingHeight', 'follow')}>{tr("Regulated height")}</button>
    <button class:on={fixed} aria-pressed={fixed} onclick={() => ed.set('OpenLaserEngravingHeight', 'fixed')}>{tr("Fixed height")}</button>
  </div>
  <p class="muted">{fixed ? tr("Measures once over the sheet, then holds Z across all engraving lines and travel. The initial measuring point must be over the sheet.") : tr("Continuously follows the sheet. The complete travel must remain over material.")}</p>
  <div class="fields big"><Field {ed} key="CutHeight" label="Engraving height" big /></div>
{/if}
<div class="section-head"><h2>{tr("Engraving lines")}</h2></div>
<div class="engrave">
  <div class="fields big">
    <Field {ed} key="OpenLaserFillSpacing" big />
    <Field {ed} key="OpenLaserFillAngle" big />
    <Field {ed} key="OpenLaserFillBidirectional" big />
    <Field {ed} key="OpenLaserFillOutline" big />
  </div>
  <FillSample {fill} />
</div>
<details>
  <summary>{tr("Passes and run-up")}</summary>
  <div class="fields big">
    <Field {ed} key="OpenLaserFillOverscan" label="Minimum laser-off run-up" note="The actual run-up is calculated from speed and acceleration, and is at least this value." big />
    <Field {ed} key="OpenLaserFillPasses" big />
    <Field {ed} key="OpenLaserFillPassAngle" big />
    <Field {ed} key="OpenLaserFillPassShift" big />
  </div>
</details>
<details>
  <summary>{tr("Assist gas and optical setup")}</summary>
  <div class="fields big">
    {#if ed.a.gas}
      <Field {ed} key="CutGasType" label="Assist gas" big />
      <Field {ed} key="CutAirPressure" label="Gas pressure" big />
    {/if}
    <Field {ed} key="OpenLaserManualFocus" big />
    <Field {ed} key="OpenLaserLens" big />
    <Field {ed} key="OpenLaserNozzleDiameter" big />
  </div>
</details>
<style>
  .engrave { display:grid; grid-template-columns:minmax(0, 1fr) 170px; gap:16px; align-items:start; }
  details { margin-top:20px; }
  summary { cursor:pointer; padding:12px 0; font-weight:600; }
  @media (max-width: 900px) { .engrave { grid-template-columns:1fr; } }
</style>
