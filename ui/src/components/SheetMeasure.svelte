<script lang="ts" module>
  import type { Measured as Kept } from '../api/client';
  /** The last measurement until taken or discarded: it outlives the panel. */
  let kept = $state<{ result: Kept | null }>({ result: null });
</script>

<script lang="ts">
  // Measuring the sheet by its edges, as the vendor's capacitive edge
  // search: from where the head stands, the head goes down and finds each
  // edge; the result shows before it is taken. Taking it lays the sheet's
  // corner and angle (or a disc's centre) and puts the job origin where
  // chosen.
  import { t } from '../i18n/index';
  import Stepper from './params/Stepper.svelte';
  import { api, type Measured, type SheetShape } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { explain } from '../lib/format';
  import { distance, unitLabel } from '../lib/units.svelte';
  import { circleThrough, type Point } from '../lib/circle';

  const doc = $derived(server.doc!);
  const connected = $derived(doc.machine.connection.state === 'connected' && doc.machine.session.homed);
  const remember = (key: string, fallback: number): number => {
    try { const v = Number(localStorage.getItem('ol-measure-' + key)); return v > 0 ? v : fallback; } catch { return fallback; }
  };
  const keep = (key: string, v: number): void => { try { localStorage.setItem('ol-measure-' + key, String(v)); } catch { /* not kept */ } };

  let shape = $state<SheetShape>('rectangle');
  let spacing = $state(remember('spacing', 100));
  let search = $state(remember('search', 200));
  let offsetX = $state(remember('offset-x', 5));
  let offsetY = $state(remember('offset-y', 5));
  let correction = $state(remember('correction', 2));
  let bore = $state(remember('bore', 30));
  let margin = $state(remember('bore-margin', 10));
  /** Where the job origin goes once taken. */
  let origin = $state<'corner' | 'offset' | 'keep'>('corner');
  let busy = $state(false);
  let result = $derived(kept.result);
  /** A bore found by hand: the edge points the operator took with the pointer. */
  let byHand = $state(true);
  let handPoints = $state<Point[]>([]);
  /** A disc or bore by three points, or four for a check that they lie on one circle. */
  let count = $state(remember('circle-points', 3) === 4 ? 4 : 3);
  const head = $derived(doc.machine.feedback?.position_mm);

  function takePoint(): void {
    if (!head) return;
    const points = [...handPoints, [head[0], head[1]] as Point];
    if (points.length < count) { handPoints = points; return; }
    handPoints = [];
    const circle = circleThrough(points);
    if (!circle) { ui.say(t('The points lie on a line: take them spread around the bore.'), true); return; }
    if (circle.off > 1) {
      ui.say(t('The points lie up to {0} mm off one circle; take them again', circle.off.toFixed(1)), true);
      return;
    }
    if (Math.abs(circle.diameter - bore) > bore * 0.1 + 2) {
      ui.say(t('The bore measured Ø {0} mm, not Ø {1} mm; check the diameter and the points', circle.diameter.toFixed(1), bore.toFixed(1)), true);
      return;
    }
    kept.result = { shape: 'bore', points, corner: null, angle_deg: null, squareness_deg: null, center: circle.center, diameter_mm: circle.diameter };
  }

  async function measure(): Promise<void> {
    const confirmed = await ui.confirm({
      title: t('Measure the sheet'),
      body: t('The head goes down over the sheet where it stands and searches the edges at 50 mm/s. Keep a hand on Stop.'),
      confirm: t('Start measuring'),
    });
    if (!confirmed) return;
    busy = true; kept.result = null;
    try { kept.result = (await api.measureSheet({ shape, spacing_mm: spacing, search_mm: search, correction_mm: [correction, correction], bore_mm: shape === 'bore' ? bore : null, bore_margin_mm: shape === 'bore' ? margin : null, four_points: count === 4 })).measured; }
    catch (e) { ui.say(explain(e), true); }
    finally { busy = false; }
  }

  /** The corner plus the offset, turned with the sheet. */
  function offsetCorner(m: Measured): [number, number] {
    const [x, y] = m.corner!;
    const a = (m.angle_deg ?? 0) * Math.PI / 180;
    return [x + Math.cos(a) * offsetX - Math.sin(a) * offsetY, y + Math.sin(a) * offsetX + Math.cos(a) * offsetY];
  }

  async function take(): Promise<void> {
    const m = result;
    if (!m) return;
    busy = true;
    try {
      if (m.corner) await api.setSheetOrigin(m.corner, m.angle_deg ?? 0);
      const at = m.center ?? (origin === 'offset' ? offsetCorner(m) : m.corner);
      if (at && (m.center || origin !== 'keep')) await api.placement({ kind: 'fixed', origin: at });
      ui.say(t('Measurement taken'));
      kept.result = null;
    } catch (e) { ui.say(explain(e), true); }
    finally { busy = false; }
  }
</script>

<section class="measure">
  <h3>{t('Measure the sheet')}</h3>
  <p class="muted">{t('Bring the head over the sheet near its front-left corner (a disc: near its middle), then start.')}</p>
  <div class="seg block" role="group" aria-label={t('What to measure')}>
    <button class:on={shape === 'rectangle'} onclick={() => (shape = 'rectangle')}>{t('Sheet · 4 points')}</button>
    <button class:on={shape === 'two_edges'} onclick={() => (shape = 'two_edges')}>{t('Sheet · 2 edges')}</button>
    <button class:on={shape === 'disc'} onclick={() => (shape = 'disc')}>{t('Disc')}</button>
    <button class:on={shape === 'bore'} onclick={() => (shape = 'bore')}>{t('Bore')}</button>
  </div>
  {#if shape === 'disc' || shape === 'bore'}
    <div class="seg block" role="group" aria-label={t('Points on the circle')}>
      <button class:on={count === 3} onclick={() => { count = 3; keep('circle-points', 3); handPoints = []; }}>{t('3 points')}</button>
      <button class:on={count === 4} onclick={() => { count = 4; keep('circle-points', 4); handPoints = []; }}>{t('4 points')}</button>
    </div>
  {/if}
  {#if shape === 'bore'}
    <Stepper label={t('Bore diameter')} value={bore} unit="mm" step={1} disabled={busy} apply={(v) => { bore = Math.min(1000, Math.max(5, v)); keep('bore', bore); }} />
    <div class="seg block" role="group" aria-label={t('How to find the bore')}>
      <button class:on={byHand} onclick={() => (byHand = true)}>{t('By hand')}</button>
      <button class:on={!byHand} onclick={() => { byHand = false; handPoints = []; }}>{t('Sensor')}</button>
    </div>
    {#if byHand}
      <p class="muted">{t('Lower the head with Z down (not Follow), then move the pointer onto the bore edge and take the point; the points spread around the edge.')}</p>
      <button class="btn btn-primary block" disabled={busy || !head} onclick={takePoint}>
        {t('Take edge point {0} of {1}', handPoints.length + 1, count)}{#if head} · X {distance(head[0])} · Y {distance(head[1])}{/if}
      </button>
      {#if handPoints.length}<button class="btn btn-ghost block" onclick={() => (handPoints = [])}>{t('Start the points again')}</button>{/if}
    {:else}
      <p class="muted">{t('Put the head roughly over the middle of the bore: the touches start on the material beside it and search toward its middle.')}</p>
      <Stepper label={t('Start beside the bore')} value={margin} unit="mm" step={1} disabled={busy} apply={(v) => { margin = Math.min(100, Math.max(1, v)); keep('bore-margin', margin); }} />
    {/if}
  {:else if shape !== 'disc'}
    <Stepper label={t('Point spacing')} value={spacing} unit="mm" step={10} disabled={busy} apply={(v) => { spacing = Math.max(5, v); keep('spacing', spacing); }} />
  {/if}
  {#if !(shape === 'bore' && byHand)}<Stepper label={t('Edge correction')} value={correction} unit="mm" step={0.5} disabled={busy} apply={(v) => { correction = Math.min(20, Math.max(0, v)); keep('correction', correction); }} />{/if}
  {#if shape !== 'bore'}<Stepper label={t('Search travel')} value={search} unit="mm" step={10} disabled={busy} apply={(v) => { search = Math.max(5, v); keep('search', search); }} />{/if}
  {#if !(shape === 'bore' && byHand)}<button class="btn btn-primary block" disabled={busy || !connected} onclick={measure}>
    {busy ? t('Measuring…') : t('Start measuring')}
  </button>{/if}
  {#if !connected}<p class="muted">{t('Connect and home the machine to measure.')}</p>{/if}

  {#if result}
    <div class="result">
      {#if result.corner}
        <p><strong>{t('Corner')}</strong> X {distance(result.corner[0])} · Y {distance(result.corner[1])} {unitLabel('mm')}</p>
        <p><strong>{t('Angle')}</strong> {(result.angle_deg ?? 0).toFixed(3)}°{#if result.squareness_deg !== null} · {t('square within {0}°', Math.abs(result.squareness_deg).toFixed(2))}{/if}</p>
        <div class="seg block" role="group" aria-label={t('Job origin')}>
          <button class:on={origin === 'corner'} onclick={() => (origin = 'corner')}>{t('Origin at the corner')}</button>
          <button class:on={origin === 'offset'} onclick={() => (origin = 'offset')}>{t('Corner + offset')}</button>
          <button class:on={origin === 'keep'} onclick={() => (origin = 'keep')}>{t('Keep the origin')}</button>
        </div>
        {#if origin === 'offset'}
          <Stepper label={t('Offset X')} value={offsetX} unit="mm" step={1} apply={(v) => { offsetX = v; keep('offset-x', v); }} />
          <Stepper label={t('Offset Y')} value={offsetY} unit="mm" step={1} apply={(v) => { offsetY = v; keep('offset-y', v); }} />
        {/if}
      {:else if result.center}
        <p><strong>{t('Centre')}</strong> X {distance(result.center[0])} · Y {distance(result.center[1])} {unitLabel('mm')}</p>
        <p><strong>Ø</strong> {distance(result.diameter_mm ?? 0)} {unitLabel('mm')} · {t('the job origin goes to the centre')}</p>
      {/if}
      <div class="row">
        <button class="btn btn-ghost" disabled={busy} onclick={() => (kept.result = null)}>{t('Discard')}</button>
        <button class="btn btn-primary" disabled={busy} onclick={take}>{t('Take measurement')}</button>
      </div>
    </div>
  {/if}
</section>

<style>
  .measure { display:grid; gap:8px; padding:10px 12px; border:1px solid var(--line); border-radius:12px; background:var(--panel-2); }
  .measure h3 { margin:0; }
  .result { display:grid; gap:6px; padding-top:8px; border-top:1px solid var(--line); }
  .result p { margin:0; font-variant-numeric:tabular-nums; }
  .row { display:grid; grid-template-columns:1fr 1fr; gap:8px; }
</style>
