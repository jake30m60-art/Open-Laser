<script lang="ts">
  import { t } from '../i18n/index';

  import { distance, quantity, unitLabel } from '../lib/units.svelte';
  // The run side: the DRO, homing and origin, and the jog pad. An axis
  // moves while its button is held and a heartbeat renews the lease;
  // anything that could lose the release, a blur, a hidden page or a
  // cancelled pointer, releases too.
  import { onMount } from 'svelte';
  import { Hold } from '../lib/hold';
  import SheetPosition from '../components/SheetPosition.svelte';
  import GoToXy from '../components/GoToXy.svelte';
  import HoldButton from '../components/HoldButton.svelte';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { osk } from '../lib/osk.svelte';
  import { explain, laserLabel } from '../lib/format';
  import { access } from '../lib/access.svelte';
  import { plain } from '../lib/plain';

  const doc = $derived(server.doc!);
  const machine = $derived(doc.machine);
  const feedback = $derived(machine.feedback);
  const origin = $derived(doc.draft?.origin ?? null);
  const readiness = $derived.by(() => {
    if (access.canControl) return doc.readiness;
    const closed = { ok: false, reason: t('Take control to move the machine') };
    return {
      ...doc.readiness, home: closed, position: closed, calibrate: closed, jog: closed,
      xy_jog: [[closed, closed], [closed, closed]], head_up: closed, head_down: closed, frame: closed,
    };
  });
  const headEnabled = $derived(doc.bindings?.head_enabled ?? false);
  const headJog = $derived(doc.bindings?.outputs.head_jog ?? false);
  const fresh = $derived(doc.machine.connection.state === 'connected' && !!feedback && feedback.age_ms <= 1000);
  const homed = $derived(fresh && doc.machine.session.homed && feedback?.referenced.every(Boolean));
  const headHomed = $derived(fresh && feedback?.head.referenced);
  const sourceLabel = $derived(doc.mode ? laserLabel(doc.mode) : t('Not selected'));

  const work = $derived.by(() => {
    if (!feedback) return null;
    const [x, y, z] = feedback.position_mm;
    return origin ? [x - origin[0], y - origin[1], z] : [x, y, z];
  });

  async function call(action: () => Promise<unknown>): Promise<void> {
    try { await action(); } catch (error) { ui.say(explain(error), true); }
  }

  type Axis = 0 | 1 | 'z' | 'w';
  let tableSpeed = $state(5);
  let auxiliary = $state<'z' | 'w'>('z');
  const tableEnabled = $derived(doc.bindings?.table_maximum_mm != null);
  const auxiliaryUp = $derived(auxiliary === 'z' ? headJog && readiness.head_up.ok : tableEnabled && readiness.jog.ok);
  const auxiliaryDown = $derived(auxiliary === 'z' ? headJog && readiness.head_down.ok : tableEnabled && readiness.jog.ok);
  function selectAuxiliary(axis: 'z' | 'w'): void { held.cancel(); auxiliary = axis; }
  const held = new Hold({
    heartbeat: (lease) => api.machine('heartbeat', { lease }),
    release: (lease) => api.machine('release', { lease }),
  }, (error) => ui.say(explain(error), true), () => server.link && access.canControl);
  onMount(() => held.mount());
  $effect(() => {
    if (!server.link || !access.canControl) {
      held.cancel();
    }
  });
  // Every key jogs only while held (DESIGN.md rule 5): the lease renews
  // while the finger stays down and the controller stops when it lifts.
  function press(event: PointerEvent, axis: Axis, positive: boolean, diagonal?: boolean): void {
    held.press(event, (lease) => axis === 'w' ? api.machine('table', { lease, table: { positive, speed_mm_s: tableSpeed } })
      : axis === 'z' ? api.machine('outputs', { lease, output: { kind: 'head_jog', up: positive, fast: ui.jogFast } })
      : api.machine('jog', { lease, jog: { axis, positive, step_mm: null, fast: ui.jogFast, diagonal } }));
  }
  /** A diagonal needs both of its directions, and never recovers a limit. */
  const diagonalOk = (xPositive: boolean, yPositive: boolean) =>
    !readiness.xy_recovery && readiness.xy_jog[0]![Number(xPositive)]!.ok && readiness.xy_jog[1]![Number(yPositive)]!.ok;
  let goingTo = $state(false);

  // Why a control is unavailable is on the Run page's status line.
  const originGate = $derived(readiness.position.ok && !origin ? { ok: false, reason: t('set a job origin first') } : readiness.position);

  const canJog = $derived(readiness.xy_jog.some(axis => axis.some(direction => direction.ok)));
  /** The head controller is following the sheet: the Follow button, or a cut. */
  /** The pendant key held down now, so its button on screen lights up. */
  const lit = (key: string, withFn = false) => doc.pendant?.key === key && (doc.pendant.with_fn === withFn || key === 'Follow' || key === 'Frame');
  const following = $derived(feedback ? [104, 117].includes(feedback.head.command) : false);
  const jogTitle = $derived(canJog ? '' : plain(readiness.jog.reason).text);
  const jogSpeed = $derived(doc.bindings?.jog_speed[ui.jogFast ? 1 : 0]);
  const jogSpeedText = $derived(jogSpeed != null
    ? quantity(readiness.xy_recovery ? Math.min(1, jogSpeed) : jogSpeed, 'mm/s')
    : t('tap to switch'));
  /** The arrow's rotation on a diagonal jog key, in degrees. */
  const cornerTurn = (xPositive: boolean, yPositive: boolean): number =>
    yPositive ? (xPositive ? 45 : -45) : (xPositive ? 135 : -135);
  function editTableSpeed(): void {
    osk.number('Table speed', tableSpeed, 'mm/s', v => { if (v > 0 && v <= 100) tableSpeed = v; });
  }
</script>

{#snippet key(axis: Axis, positive: boolean, icon: string, label: string, enabled: boolean, pendant: boolean)}
  <button class:lit={pendant} onpointerdown={(event) => press(event, axis, positive)} disabled={!enabled}><i class="ic {icon}"></i><small>{t(label)}</small></button>
{/snippet}
{#snippet corner(xPositive: boolean, yPositive: boolean)}
  {@const label = `${xPositive ? 'X+' : 'X−'} ${yPositive ? 'Y+' : 'Y−'}`}
  <button
    class="diagonal"
    style:--turn="{cornerTurn(xPositive, yPositive)}deg"
    aria-label="Jog {t(label)}"
    onpointerdown={(event) => press(event, 0, xPositive, yPositive)}
    disabled={!diagonalOk(xPositive, yPositive)}
  ><i class="ic ic-arrow-up"></i></button>
{/snippet}

<aside class="panel side run-side">
 <div class="run-side-scroll">
  {#if machine.program?.state === 'held' && doc.recovery?.pause_position}
    <div class="pause-position" role="status">
      <strong>{t('Paused at X {0} · Y {1} {2}', distance(doc.recovery.pause_position[0]), distance(doc.recovery.pause_position[1]), unitLabel('mm'))}</strong>
      <span>{t("You can move the head. Resume returns to this saved position.")}</span>
    </div>
  {/if}
  {#if !machine.program || !['running', 'finishing', 'held'].includes(machine.program.state)}<SheetPosition compact />{/if}
  <div class="dro">
    {#each [['X', 0], ['Y', 1], ['Z', 2]] as [name, i]}
      {@const reading = work ? distance(work[i as number]!) : '—'}
      <div class="cell" class:off={!work}>
        <div class="ax"><span>{name} <small>{unitLabel('mm')}</small></span></div>
        <div class="val" style:--characters={Math.max(6, reading.length)}>{reading}</div>
        <div class="mach">{#if feedback}M {distance(feedback.position_mm[i as number]!)}{:else}—{/if}</div>
      </div>
    {/each}
  </div>

  <div class="row motion-actions">
    <HoldButton
      class="btn btn-move"
      onhold={() => call(() => api.machine('home'))}
      disabled={!readiness.home.ok}
      title={plain(readiness.home.reason).text}
    ><i class="ic ic-home"></i>{t("Home")}</HoldButton>
    <HoldButton
      class="btn btn-move {lit('GoOrigin', true) ? 'lit' : ''}"
      onhold={() => call(() => api.machine('go-origin', { fast: ui.jogFast }))}
      disabled={!originGate.ok}
      title={plain(originGate.reason).text}
    >{t("Go origin")}</HoldButton>
    <HoldButton
      class="btn btn-move {lit('ZCalibrate', true) ? 'lit' : ''}"
      label={t("Calibrate Z")}
      onhold={() => call(() => api.machine('calibrate'))}
      disabled={!readiness.calibrate.ok}
      title={plain(readiness.calibrate.reason).text}
    >{t("Calibrate Z")}</HoldButton>
    {#if following}
      <button class="btn btn-move follow on" class:lit={lit('Follow')} onclick={() => call(() => api.machine('raise-head'))}
        title={t("The head follows the sheet; tap to raise it")}><i class="ic ic-arrow-up"></i>{t("Head up")}</button>
    {:else}
      <HoldButton
        class="btn btn-move follow {lit('Follow') ? 'lit' : ''}"
        label={t("Follow")}
        onhold={() => call(() => api.machine('follow'))}
        disabled={!readiness.calibrate.ok}
        title={readiness.calibrate.ok ? t("Hold: the head follows the sheet at the machine follow height") : plain(readiness.calibrate.reason).text}
      ><i class="ic ic-arrow-down"></i>{t("Follow")}</HoldButton>
    {/if}
  </div>
  <div class="frame-row">
    <HoldButton
      class="btn btn-move frame-action {lit('Frame') ? 'lit' : ''}"
      onhold={() => call(() => api.machine('frame'))}
      disabled={!readiness.frame.ok}
      title={plain(readiness.frame.reason).text}
    ><i class="ic ic-frame"></i>{t("Frame · laser off")}</HoldButton>
    <button class="btn btn-ghost go-xy" disabled={!readiness.position.ok} title={plain(readiness.position.reason).text} onclick={() => (goingTo = true)}>
      {t("Go to X/Y…")}
    </button>
  </div>

  <div class="jogblock">
    <div class="jog" class:disabled={!canJog} title={jogTitle}>
      {@render corner(false, true)}{@render key(1, true, 'ic-arrow-up', 'Y+', readiness.xy_jog[1]![1]!.ok, lit('Up'))}{@render corner(true, true)}
      {@render key(0, false, 'ic-arrow-left', 'X−', readiness.xy_jog[0]![0]!.ok, lit('Left'))}
      <button class="hub" class:lit={lit('FastStep')} onclick={() => (ui.jogFast = !ui.jogFast)} title={t("Tap to switch speed")}>
        <strong>{ui.jogFast ? t('Fast') : t('Slow')}</strong>
        <small>{jogSpeedText}</small>
      </button>
      {@render key(0, true, 'ic-arrow-right', 'X+', readiness.xy_jog[0]![1]!.ok, lit('Right'))}
      {@render corner(false, false)}{@render key(1, false, 'ic-arrow-down', 'Y−', readiness.xy_jog[1]![0]!.ok, lit('Down'))}{@render corner(true, false)}
    </div>
    <div class="zcol">
      {@render key(auxiliary, true, 'ic-arrow-up', auxiliary === 'z' ? 'Z up' : 'W+', auxiliaryUp, lit('ZUp', auxiliary === 'w'))}
      <button
        class="hub"
        onclick={() => selectAuxiliary(auxiliary === 'z' ? 'w' : 'z')}
        aria-label={t(auxiliary === 'z' ? 'Z head; switch to W table' : 'W table; switch to Z head')}
        title={auxiliary === 'z' ? t('Tap for W table') : t('Tap for Z head')}
      >
        <strong>{auxiliary.toUpperCase()}</strong>
        <small>{auxiliary === 'z' ? t('Head') : t("Table")}</small>
        <small>{t('Tap for {0}', auxiliary === 'z' ? 'W' : 'Z')}</small>
      </button>
      {@render key(auxiliary, false, 'ic-arrow-down', auxiliary === 'z' ? 'Z down' : 'W−', auxiliaryDown, lit('ZDown', auxiliary === 'w'))}
    </div>
  </div>
  {#if readiness.xy_recovery}
    <p class="muted auxiliary-hint" role="status">
      {t("X/Y limit recovery: hold an available direction to move away up to 1 mm at 1 mm/s or slower. Release between presses. Once clear, use Home.")}
    </p>
  {/if}
  {#if auxiliary === 'w'}
    {#if tableEnabled}
      <div class="table-readout">
        <span>W <strong>{quantity(feedback?.table_mm, 'mm')}</strong></span>
        <button class="btn btn-ghost" onclick={editTableSpeed}>{quantity(tableSpeed, 'mm/s')}</button>
      </div>
    {:else}<p class="muted auxiliary-hint">{t("W table motion is not enabled in the loaded machine configuration.")}</p>{/if}
  {:else if !headEnabled}<p class="muted auxiliary-hint">{t("No head controller configured.")}</p>
  {:else if readiness.head_recovery}
    <p class="muted auxiliary-hint" role="status">
      {t("Z limit recovery: hold the available direction to move up to 1 mm at 1 mm/s or slower. Release between presses. Once clear, use Home.")}
    </p>
  {/if}
 </div>
</aside>
{#if goingTo}<GoToXy onclose={() => (goingTo = false)} />{/if}

<style>
  .run-side { padding:12px; }
  /* The button whose key is held on the pendant. */
  .run-side :global(.lit) { outline:3px solid var(--accent); outline-offset:-3px; background:var(--accent-soft); color:var(--accent-2); }
  .pause-position { display:flex; flex-direction:column; gap:5px; padding:10px; border:1px solid var(--hold); border-radius:9px; font-size:var(--t-sm); }
  .pause-position span { color:var(--ink-3); line-height:1.4; }
  .run-side-scroll { flex:1; min-height:0; overflow-y:auto; display:flex; flex-direction:column; gap:8px; padding-right:2px; }
  .run-side-scroll > :global(*) { flex-shrink:0; }
  .motion-actions :global(.btn) { min-width:0; padding:0 6px; font-size:var(--t-sm); }
  .motion-actions { display:grid; grid-template-columns:repeat(4, minmax(0, 1fr)); gap:8px; }
  .motion-actions :global(.follow.on) { background:var(--accent); color:var(--accent-ink); border-color:transparent; }
  .dro .cell { container-type:inline-size; padding:6px 8px; height:76px; gap:1px; }
  .dro .val { font-size:max(var(--t-sm), min(var(--t-xl), calc(100cqw / var(--characters) / .64))); line-height:1.15; overflow:visible; white-space:nowrap; }
  .dro .mach { font-size:var(--t-sm); line-height:1.2; color:var(--ink-3); text-align:right; font-variant-numeric:tabular-nums; white-space:nowrap; }
  .run-side-scroll :global(.frame-action) { width:100%; min-height:52px; font-size:var(--t-base); }
  .jogblock { flex:1 0 204px; display: grid; grid-template-columns: minmax(0, 3fr) minmax(88px, 1fr); gap: 12px; }
  .jog { height: auto; grid-template-rows:repeat(3,minmax(64px,1fr)); }
  .zcol { grid-template-columns: minmax(0, 1fr); grid-template-rows:repeat(3,minmax(64px,1fr)); }
  .zcol > * { grid-column: 1; }
  .auxiliary-hint { font-size: var(--t-sm); color: var(--ink-3); }
  .frame-row { display: grid; grid-template-columns: minmax(0, 2fr) minmax(0, 1fr); gap: 8px; }
  .frame-row .go-xy { min-height: 52px; }
  .jog .diagonal .ic { transform: rotate(var(--turn)); }
  .table-readout { display: flex; justify-content: space-between; align-items: center; gap: 12px; }
</style>
