<script lang="ts">
  import { t } from '../../../i18n/index';

  // Flying cut: the straight edges of the selected contours that lie on one
  // line are cut in one pass, the laser off over the webs between them.
  // Circles and other closed contours can fly whole: each entered at speed
  // from a laser-off approach and closed with a small overlap.
  import Stepper from '../../../components/params/Stepper.svelte';
  import Toggle from '../../../components/params/Toggle.svelte';
  import { defaultFly } from '../../../lib/features';
  import { plural } from '../../../lib/format';
  import { ui } from '../../../stores/ui.svelte';
  import type { Features } from '../../../api';
  import type { SetFeatures } from './form';

  let { features, on, selectedContours, error, set }: {
    features: Features;
    on: boolean;
    selectedContours: number[];
    /** Why the drawing could not be prepared, if it could not. */
    error: string | null;
    set: SetFeatures;
  } = $props();

  const fly = $derived(features.fly ?? defaultFly(selectedContours));
  const summary = $derived(features.fly
    ? fly.circles || fly.outlines
      ? t('{0} cut flying.', plural(fly.contours.length, 'contour'))
      : t('{0} cut flying. Switch on circles or contours to fly shapes with arcs too.', plural(fly.contours.length, 'contour'))
    : t('Select the contours on the drawing, such as a grid of square holes, then switch on the flying cut.'));
  const useSelected = () => set(f => { if (f.fly) f.fly.contours = [...selectedContours]; });
  const setRunUp = (v: number) => set(f => { f.fly!.run_up = Math.min(50, v); });
  const setOvercut = (v: number) => set(f => { f.fly!.overcut = Math.min(2, v); });
  const setTolerance = (v: number) => set(f => { f.fly!.tolerance = Math.min(1, Math.max(0.001, v)); });
  const setOneWay = (v: boolean) => set(f => { f.fly!.one_way = v; });
  const setFar = (v: boolean) => set(f => { f.fly!.from_far_side = v; });
  const setReversed = (v: boolean) => set(f => { f.fly!.directions_reversed = v; });
  const setCircles = (v: boolean) => set(f => { f.fly!.circles = v; });
  const setOutlines = (v: boolean) => set(f => { f.fly!.outlines = v; });
  const setOverlap = (v: number) => set(f => { f.fly!.overlap = Math.min(5, Math.max(0, v)); });
</script>

<div class="param"><div class="lbl">{summary}</div></div>
<button class="btn btn-soft" disabled={!on || selectedContours.length < 1 || !!ui.picking} onclick={useSelected}>{t("Use selected contours (")}{selectedContours.length})</button>
<Stepper label={t("Run-up, laser off")} value={fly.run_up} unit="mm" step={0.5} disabled={!on} apply={setRunUp} />
<p class="fly-hint">{t("The head reaches speed before the beam comes on, so a line never pierces standing. 0 pierces at the first cut as usual. Keep the run-up on the sheet: the height sensor needs metal under it.")}</p>
<Stepper label={t("Overcut at corners")} value={fly.overcut} unit="mm" step={0.05} disabled={!on} apply={setOvercut} />
<p class="fly-hint">{t("Runs each cut a little past its ends so crossing lines separate the corner; never closer than 0.5 mm to the next cut.")}</p>
<h3 class="fly-head">{t("Circles and contours")}</h3>
<Toggle label={t("Fly circles")} value={fly.circles ?? false} disabled={!on} apply={setCircles} />
<p class="fly-hint">{t("Round holes are entered on a tangent at cutting speed, the beam coming on at the edge. The holes of a row are joined in one movement, row by row.")}</p>
<Toggle label={t("Fly contours with arcs")} value={fly.outlines ?? false} disabled={!on} apply={setOutlines} />
<p class="fly-hint">{t("Any other closed contour starts from a laser-off approach at cutting speed and runs on to the next without stopping. Holes fly before the outlines around them.")}</p>
<Stepper label={t("Overlap")} value={fly.overlap ?? 0.3} unit="mm" step={0.05} disabled={!on || !(fly.circles || fly.outlines)} apply={setOverlap} />
<p class="fly-hint">{t("How far a flying circle or contour cuts past its start, so it closes. Only for thin sheet: without a standing pierce the beam has to go through at once.")}</p>
<h3 class="fly-head">{t("Passes")}</h3>
<Toggle label={t("Every pass the same way")} value={fly.one_way} disabled={!on} apply={setOneWay} />
<p class="fly-hint">{t("Off, passes alternate so each starts where the last ended. On, every cut runs the same way, which some sheets cut more evenly.")}</p>
<Toggle label={t("Start from the far side")} value={fly.from_far_side} disabled={!on} apply={setFar} />
<Toggle label={t("Other direction first")} value={fly.directions_reversed} disabled={!on} apply={setReversed} />
<p class="fly-hint">{t("For example vertical lines before horizontal ones. The cut order preview shows the passes in turn.")}</p>
<Stepper label={t("Same-line tolerance")} value={fly.tolerance} unit="mm" step={0.01} disabled={!on} apply={setTolerance} />
<p class="fly-hint">{t("For thin sheet: piercing at full speed needs material the beam goes through at once. Try it on a test piece first.")}</p>
{#if error}<p class="warn-text">{error}</p>{/if}

<style>
  .fly-head { margin:10px 0 0; }
  .fly-hint { margin:-4px 0 6px; font-size:var(--t-sm); color:var(--ink-3); }
</style>
