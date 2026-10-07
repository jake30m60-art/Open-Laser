<script lang="ts">
  // A marked layer's scan fill: a sample of its lines at scale, the lines'
  // spacing, angle, run-up and passes, and what the machine adds to them:
  // the run-up its speed needs and the backlash the engraving calibration
  // measured. The calibration table itself lives in Settings only.
  import { t as tr } from '../../i18n/index';
  import Stepper from '../../components/params/Stepper.svelte';
  import Toggle from '../../components/params/Toggle.svelte';
  import FillSample from '../../components/FillSample.svelte';
  import { scanRunUp } from '../../lib/scan-calibration';
  import { defaultImage } from '../../lib/images';
  import { ui } from '../../stores/ui.svelte';
  import type { Fill, ImageFill, ImageMode } from '../../api';

  let { fill, apply, speed = null, offset = null, image = false }: {
    fill: Fill;
    apply: (change: Partial<Fill>) => void;
    /** The layer's engraving speed in mm/s, when its recipe gives one. */
    speed?: number | null;
    /** The calibrated compensation at that speed and angle, or none. */
    offset?: number | null;
    /** Whether the layer engraves a picture: its shading settings show. */
    image?: boolean;
  } = $props();
  const shading = $derived(fill.image ?? defaultImage());
  const shade = (change: Partial<ImageFill>) => apply({ image: { ...shading, ...change } });
  const MODES: Array<[ImageMode, string, string]> = [
    ['grayscale', 'Grayscale', 'Power follows the gray: dark parts with the most power, light ones with the least.'],
    ['dither', 'Dithering', 'Dots at full power, spread so their density gives the gray. Often the clearer choice on metal.'],
    ['threshold', 'Black and white', 'Full power where darker than middle gray, nothing elsewhere: for logos and line art.'],
  ];

  const needed = $derived(speed ? Math.ceil(scanRunUp(speed)) : null);
  const short = $derived(needed !== null && fill.run_up + 1e-9 < needed);
  const fmt = (n: number, d = 2) => Number(n.toFixed(d)).toString();
  type Passes = { passes: number; pass_angle: number; pass_shift: number };
  const PRESETS: Array<[string, Passes]> = [
    ['Single', { passes: 1, pass_angle: 0, pass_shift: 0 }],
    ['Crosshatch', { passes: 2, pass_angle: 90, pass_shift: 0 }],
    ['Interleaved', { passes: 2, pass_angle: 0, pass_shift: 0.5 }],
    ['Deeper ×3', { passes: 3, pass_angle: 0, pass_shift: 0 }],
  ];
  /** Whether the fill's passes are this preset's: shown lit, as the choice in force. */
  const matches = (p: Passes) => fill.passes === p.passes && (p.passes === 1 || (Math.abs(fill.pass_angle - p.pass_angle) < 1e-6 && Math.abs(fill.pass_shift - p.pass_shift) < 1e-6));
  function calibration(): void {
    ui.machinePage = 6;
    ui.tab = 'machine';
  }
</script>

<div class="fill">
  {#if image}
    <h4 class="fill-head">{tr("Picture")}</h4>
    <div class="presets" role="group" aria-label={tr("How the picture is engraved")}>
      {#each MODES as [mode, label] (mode)}
        <button class="chip" class:on={shading.mode === mode} aria-pressed={shading.mode === mode} onclick={() => shade({ mode })}>{tr(label)}</button>
      {/each}
    </div>
    <p class="fill-hint">{tr(MODES.find(([m]) => m === shading.mode)?.[2] ?? '')}</p>
    {#if shading.mode === 'grayscale'}
      <Stepper label={tr("Power for light gray")} value={shading.min_power} unit="%" step={5} apply={(v) => shade({ min_power: Math.min(shading.max_power, Math.max(0, v)) })} />
    {/if}
    <Stepper label={tr("Power for black")} value={shading.max_power} unit="%" step={5} apply={(v) => shade({ max_power: Math.min(100, Math.max(shading.min_power, v)) })} />
    <p class="fill-hint">{tr("In percent of the layer recipe's power. White is never engraved.")}</p>
    <Stepper label={tr("Gamma")} value={shading.gamma} unit="" step={0.1} apply={(v) => shade({ gamma: Math.min(10, Math.max(0.1, +v.toFixed(2))) })} />
    <p class="fill-hint">{tr("Above 1 engraves the midtones lighter, below 1 darker.")}</p>
    <Toggle label={tr("Invert: engrave the light parts")} value={shading.invert} apply={(v) => shade({ invert: v })} />
    <p class="fill-hint">{tr("The line spacing is also the size of one dot along the line.")}</p>
  {/if}
  <FillSample {fill} />
  <Stepper label={tr("Line spacing")} value={fill.spacing} unit="mm" step={0.01} apply={(v) => apply({ spacing: Math.min(20, Math.max(0.01, v)) })} />
  <Stepper label={tr("Angle")} value={fill.angle} unit="°" step={15} apply={(v) => apply({ angle: ((v % 180) + 180) % 180 })} />
  <Stepper label={tr("Overscan, laser off")} value={fill.run_up} unit="mm" step={0.5} apply={(v) => apply({ run_up: Math.min(1000, Math.max(0, v)) })} />
  {#if short && needed !== null && speed}
    <p class="warn-text fill-hint">{tr("At {0} mm/s the head needs about {1} mm to reach speed; shorter, the line ends are engraved while it still speeds up.", fmt(speed, 0), needed)}
      <button class="link" onclick={() => apply({ run_up: needed })}>{tr("Set {0} mm", needed)}</button></p>
  {:else}
    <p class="fill-hint">{tr("How far every line runs past the engraving, so the head turns round off the work and engraves at full speed.")}</p>
  {/if}
  <Toggle label={tr("Both directions")} value={fill.bidirectional} apply={(v) => apply({ bidirectional: v })} />
  <p class="fill-hint">{tr("Off engraves every line under the same conditions: one axis, one direction, one speed, whatever the shape's strokes. The head returns with the laser off.")}</p>
  <p class="fill-hint calib">
    {#if offset !== null && speed}
      {tr("Backlash compensation: {0} mm each way at {1} mm/s, from the engraving calibration.", fmt(offset, 3), fmt(speed, 0))}
    {:else}
      {tr("No engraving calibration for this laser yet: lines of the two directions may not line up.")}
    {/if}
    <button class="link" onclick={calibration}>{tr("Engraving calibration")}</button>
  </p>
  <h4 class="fill-head">{tr("Passes")}</h4>
  <Stepper label={tr("Passes")} value={fill.passes} unit="" step={1} apply={(v) => apply({ passes: Math.min(20, Math.max(1, Math.round(v))) })} />
  <Stepper label={tr("Turn each pass")} value={fill.pass_angle} unit="°" step={15} apply={(v) => apply({ pass_angle: ((v % 360) + 360) % 360 })} />
  <Stepper label={tr("Shift each pass")} value={Math.round(fill.pass_shift * 100)} unit="%" step={25} apply={(v) => apply({ pass_shift: Math.min(99, Math.max(0, v)) / 100 })} />
  <p class="fill-hint">{tr("2 passes, 90° crosshatch. 2 passes, 0°, 50 % lays the second pass between the first one's lines: finer, without a slower line. 3 passes, 0°, 0 % engraves deeper.")}</p>
  <div class="presets">
    {#each PRESETS as [label, preset] (label)}
      <button class="chip" class:on={matches(preset)} aria-pressed={matches(preset)} onclick={() => apply(preset)}>{tr(label)}</button>
    {/each}
  </div>
  <Toggle label={tr("Trace the outlines after the fill")} value={fill.outline} apply={(v) => apply({ outline: v })} />
</div>

<style>
  .fill { display:grid; gap:6px; padding:0 0 6px; }
  .fill-hint { margin:-2px 0 4px; font-size:var(--t-sm); color:var(--ink-3); }
  .fill-hint.warn-text { color:var(--warn); }
  .calib { display:flex; flex-wrap:wrap; gap:6px; align-items:baseline; }
  .fill-head { margin:8px 0 0; font-size:var(--t-sm); color:var(--ink-2); }
  .presets { display:flex; flex-wrap:wrap; gap:6px; }
  .presets .chip { min-height:44px; }
</style>
