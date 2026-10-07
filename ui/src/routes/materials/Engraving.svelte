<script lang="ts">
  import { t as tr } from '../../i18n/index';

  // An engraving recipe's scan fill: what a marked layer engraves with when
  // the recipe is chosen for it. Speed, power and frequency are the
  // Cutting page's; these are the lines between them.
  import Field from './Field.svelte';
  import { fillOf } from '../../lib/engraving';
  import FillSample from '../../components/FillSample.svelte';
  import type { Editor } from '../../lib/recipe';

  let { ed }: { ed: Editor } = $props();
  const fill = $derived(fillOf(ed.values));
</script>

<div class="section-head"><div><h2>{tr("Engraving")}</h2><p class="muted">{tr("The lines a marked layer engraves with this recipe. Speed, power and frequency are on the Cutting tab.")}</p></div></div>
<div class="engrave">
  <div class="fields big">
    <Field {ed} key="OpenLaserFillSpacing" big />
    <Field {ed} key="OpenLaserFillAngle" big />
    <Field {ed} key="OpenLaserFillOverscan" big />
    <Field {ed} key="OpenLaserFillPasses" big />
    <Field {ed} key="OpenLaserFillPassAngle" big />
    <Field {ed} key="OpenLaserFillPassShift" big />
    <Field {ed} key="OpenLaserFillBidirectional" big />
    <Field {ed} key="OpenLaserFillOutline" big />
  </div>
  <FillSample {fill} />
</div>

<style>
  .engrave { display:grid; grid-template-columns:minmax(0, 1fr) 170px; gap:16px; align-items:start; }
  @media (max-width: 900px) { .engrave { grid-template-columns:1fr; } }
</style>
