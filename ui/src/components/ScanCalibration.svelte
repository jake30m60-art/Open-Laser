<script lang="ts" module>
  import { materialLabel } from '../lib/materials';
  let session: { speeds: number[]; angle: number; threeAngles: boolean; length: number; spacing: number; origin: [number, number] | null; recipeId: string | null } | null = null;
</script>
<script lang="ts">
  import { t as tl } from '../i18n/index';
  // Engraving calibration: how far engraved lines land behind where they
  // were meant to, per laser, scan angle and speed. The test engraves, per
  // speed, one four-line group for each direction, scanned both ways
  // without compensation: at every block's edge the ends of the lines run
  // one way stand apart from the ends of the lines run back. That shift is
  // measured and entered speed by speed; fills then take the offset for
  // their speed and angle, as Mlaser's table does. The table lives here only.
  import { foldedScanAngle, scanTestLayout, scanEnvelope, envelopeFits } from '../lib/scan-calibration';
  import { t as tr } from '../i18n/index';
  import { api } from '../api/client';
  import { scanOffsets } from '../stores/scan-offsets.svelte';
  import type { LaserMode, RecipeView, ScanOffsetsView } from '../api';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { osk } from '../lib/osk.svelte';
  import { explain, laserLabel } from '../lib/format';
  import { isEngraving } from '../lib/engraving';

  /** As the server lays the test out: speeds per row, lines per block, blocks per speed. */
  const LINES = 4, BLOCKS = 1;
  const steps = [tr("Engrave test"), tr("Measure"), tr("Table")];

  let mode = $state<LaserMode>(server.doc?.mode ?? 'fiber');
  let data = $state<ScanOffsetsView | null>(null);
  let page = $state(0);
  let busy = $state(false);
  let error = $state('');
  let saved = $state('');
  let zoomTest = $state(false);

  // The test: its speeds, angle, lines and where it lies on the bed.
  let speeds = $state<number[]>(session?.speeds ?? [200, 250, 300, 350]);
  let angle = $state(session?.angle ?? 0);
  let threeAngles = $state(session?.threeAngles ?? true);
  const testAngles = $derived(threeAngles ? [0, 45, 90] : [foldedScanAngle(angle)]);
  let length = $state(session?.length ?? 10);
  let spacing = $state(session?.spacing ?? 1);
  let origin = $state<[number, number] | null>(session?.origin ?? null);
  let recipeId = $state<string | null>(session?.recipeId ?? null);
  /** The speed being measured. */
  let current = $state(0);

  const recipes = $derived((server.doc?.library.recipes ?? []).filter((r) => r.laser === mode && isEngraving(r)));
  const recipe = $derived<RecipeView | null>(recipes.find((r) => r.id === recipeId) ?? recipes[0] ?? null);
  $effect(() => { if (!recipes.some(r => r.id === recipeId)) recipeId = recipes[0]?.id ?? null; });
  /** The angle as stored: 0 to under 180 degrees. */
  const folded = foldedScanAngle;
  const SAME = 0.05;
  const fmt = (n: number, d = 2) => Number(n.toFixed(d)).toString();

  /** One speed's blocks turned to the angle, lower-left bound at `corner`: rectangles of four lines. */
  function blocks(corner: [number, number], scanAngle: number): Array<{ lines: Array<[number, number, number, number]>; box: string }> {
    const radians = (folded(scanAngle) * Math.PI) / 180, c = Math.cos(radians), s = Math.sin(radians);
    const turn = (x: number, y: number): [number, number] => [x * c - y * s, x * s + y * c];
    const local = Array.from({ length: BLOCKS }, (_, i) => {
      const x = i * length * 1.5;
      return { x, corners: [[x, 0], [x + length, 0], [x + length, LINES * spacing], [x, LINES * spacing]] as Array<[number, number]> };
    });
    const all = local.flatMap((b) => b.corners.map(([x, y]) => turn(x, y)));
    const minX = Math.min(...all.map((p) => p[0])), minY = Math.min(...all.map((p) => p[1]));
    const at = (x: number, y: number): [number, number] => { const [u, v] = turn(x, y); return [corner[0] + u - minX, corner[1] + v - minY]; };
    return local.map((b) => ({
      box: b.corners.map(([x, y]) => at(x, y).join(',')).join(' '),
      lines: Array.from({ length: LINES }, (_, k) => {
        const y = (k + 0.5) * spacing;
        return [...at(b.x, y), ...at(b.x + length, y)] as [number, number, number, number];
      }),
    }));
  }
  const layout = $derived(scanTestLayout(origin ?? [0, 0], speeds, testAngles, length, spacing, data?.acceleration, data?.acceleration_time));
  const tests = $derived(layout.tests);
  const before = $derived(layout.before);
  const after = $derived(layout.after);
  const envelopes = $derived(tests.map(test => scanEnvelope(test.corner, test.size, test.angle, test.speed, data?.acceleration, data?.acceleration_time)));
  const testSize = $derived<[number, number]>([
    Math.max(...tests.map(test => test.corner[0] + test.size[0])) - (origin?.[0] ?? 0),
    Math.max(...tests.map(test => test.corner[1] + test.size[1])) - (origin?.[1] ?? 0),
  ]);
  const bed = $derived(data?.bed ?? null);
  const bedView = $derived.by(() => {
    if (!bed) return '0 0 100 100';
    if (zoomTest && origin) {
      const target = tests[Math.min(current, tests.length - 1)]!;
      const selected = page === 1 ? [target] : tests;
      const bounds = selected.map(t => ({ min: { x: t.corner[0], y: t.corner[1] }, max: { x: t.corner[0] + t.size[0], y: t.corner[1] + t.size[1] } }));
      const minX = Math.min(...bounds.map(e => e.min.x)), maxX = Math.max(...bounds.map(e => e.max.x));
      const minY = Math.min(...bounds.map(e => e.min.y)), maxY = Math.max(...bounds.map(e => e.max.y));
      return [minX - 35, bed.max.y + bed.min.y - maxY - 25, maxX - minX + 55, maxY - minY + 50].join(' ');
    }
    return `${bed.min.x - 10} ${bed.min.y - 10} ${bed.max.x - bed.min.x + 20} ${bed.max.y - bed.min.y + 20}`;
  });
  const fits = $derived(!!bed && !!origin && envelopes.every(e => envelopeFits(e, bed)));
  const active = $derived(tests[Math.min(current, tests.length - 1)]!);
  const measureAngle = $derived(active.angle);
  const atAngle = $derived((data?.entries ?? []).filter(e => Math.abs(e.angle - measureAngle) < SAME));
  const angles = $derived([...new Set((data?.entries ?? []).map(e => e.angle))].sort((a, b) => a - b));
  const savedAt = (speed: number, scanAngle = measureAngle) => data?.entries.find(e => Math.abs(e.angle - scanAngle) < SAME && Math.abs(e.speed - speed) < 1e-6)?.offset ?? null;
  const measured = $derived(tests.filter(test => savedAt(test.speed, test.angle) !== null).length);
  const speed = $derived(active.speed);

  $effect(() => {
    const laser = mode;
    let live = true;
    data = null;
    error = '';
    api.scanOffsets(laser).then((view) => {
      if (!live) return;
      data = view;
      origin ??= [view.bed.min.x + 10 + before[0], view.bed.min.y + 10 + before[1]];
    }).catch((e) => { if (live) error = explain(e); });
    return () => { live = false; };
  });

  async function run(action: () => Promise<void>): Promise<void> {
    if (busy) return;
    busy = true; error = ''; saved = '';
    try { await action(); } catch (e) { error = explain(e); } finally { busy = false; }
  }
  const reload = async () => { data = await api.scanOffsets(mode); scanOffsets.refresh(mode); };

  function addSpeed(): void {
    osk.number(tr("Test speed"), speeds.length ? speeds.at(-1)! + 50 : 100, 'mm/s', (v) => {
      if (!(v > 0 && v <= 2000)) { error = 'Enter a speed from 1 to 2000 mm/s.'; return; }
      if (speeds.length >= 24) { error = 'A test holds up to 24 speeds.'; return; }
      if (!speeds.includes(v)) speeds = [...speeds, v].sort((a, b) => a - b);
    });
  }
  function editSpeed(i: number): void {
    osk.number(tr("Speed {0}", i + 1), speeds[i]!, 'mm/s', (v) => {
      if (!(v > 0 && v <= 2000)) { error = 'Enter a speed from 1 to 2000 mm/s.'; return; }
      speeds = speeds.map((s, k) => (k === i ? v : s)).filter((s, k, all) => all.indexOf(s) === k).sort((a, b) => a - b);
    });
  }
  const removeSpeed = (i: number) => { if (speeds.length > 1) { speeds = speeds.filter((_, k) => k !== i); current = 0; } };
  function number(label: string, value: number, unit: string, set: (v: number) => void, ok: (v: number) => boolean, why: string): void {
    osk.number(tr(label), value, unit, (v) => { if (ok(v)) set(v); else error = tr(why); });
  }

  /** A click on the bed puts the test's lower-left corner there. */
  function place(event: MouseEvent): void {
    if (!bed) return;
    const svg = event.currentTarget as SVGSVGElement;
    const point = svg.createSVGPoint();
    point.x = event.clientX; point.y = event.clientY;
    const at = point.matrixTransform(svg.getScreenCTM()!.inverse());
    // The diagram draws Y up: the front of the bed at the bottom.
    const x = Math.round(at.x), y = Math.round(bed.max.y + bed.min.y - at.y - testSize[1]);
    origin = [Math.max(bed.min.x + before[0], Math.min(bed.max.x - testSize[0] - after[0], x)), Math.max(bed.min.y + before[1], Math.min(bed.max.y - testSize[1] - after[1], y))];
  }
  /** Arrow keys move the test by 10 mm. */
  function nudge(event: KeyboardEvent): void {
    const step: Record<string, [number, number]> = { ArrowLeft: [-10, 0], ArrowRight: [10, 0], ArrowUp: [0, 10], ArrowDown: [0, -10] };
    const by = step[event.key];
    if (!by || !origin) return;
    event.preventDefault();
    origin = [origin[0] + by[0], origin[1] + by[1]];
  }
  function moveOrigin(axis: 0 | 1): void {
    if (!origin) return;
    number(tr("Test corner {0}", axis ? 'Y' : 'X'), origin[axis], 'mm', (v) => { origin = axis ? [origin![0], v] : [v, origin![1]]; }, () => true, '');
  }

  function loadTest(): Promise<void> {
    return run(async () => {
      if (!recipe) throw new Error(tr("Add a recipe for this laser first."));
      if (!origin || !fits) throw new Error(tr("The test does not fit on the bed there."));
      await api.scanCoupon({ recipe: recipe.id, angle, angles: $state.snapshot(testAngles), speeds: $state.snapshot(speeds), origin: $state.snapshot(origin), length, spacing });
      session = { speeds: $state.snapshot(speeds), angle, threeAngles, length, spacing, origin: $state.snapshot(origin), recipeId: recipe.id };
      current = 0;
      ui.say(tr("Test loaded: {0} speeds, angles {1}. Engrave it, then enter each measurement here.", speeds.length, testAngles.map(a => `${a}°`).join(' / ')));
      ui.setupPanel = null;
      ui.tab = 'setup';
    });
  }

  /** Enters the shift measured at `speed`, saves it with what this angle has, and moves on. */
  function measure(): void {
    const at = speed, measuredAngle = measureAngle;
    const before = savedAt(at, measuredAngle);
    osk.number(tr("{0} mm/s at {1}° · shift between → and ← lines", fmt(at, 1), measuredAngle), before === null ? 0 : before * 2, 'mm', (v) => {
      if (!(Number.isFinite(v) && v >= -4 && v <= 4)) { error = tr('Enter a shift from -4 to 4 mm.'); return; }
      void run(async () => {
        if (!data) return;
        // Every speed this angle has keeps its value; this one takes the new.
        const table = new Map(atAngle.map((e) => [e.speed, e.offset] as [number, number]));
        for (const [s] of table) if (Math.abs(s - at) < 1e-6) table.delete(s);
        table.set(at, v / 2);
        await api.saveScanOffsets({ revision: data.revision, mode, angle: measuredAngle, measured: [...table].sort((a, b) => a[0] - b[0]) });
        await reload();
        saved = tr("{0} mm/s at {1}° saved: offset {2} mm each way.", fmt(at, 1), measuredAngle, fmt(v / 2, 3));
        if (current < tests.length - 1) current += 1;
        else page = 2;
      });
    });
  }
  async function clearAngle(a: number): Promise<void> {
    if (!data) return;
    const ok = await ui.confirm({ title: tr("Delete the {0}° table?", fmt(a, 1)), body: tr("Fills at this angle are then engraved with the nearest angle that has one, or without compensation."), confirm: tr("Delete"), danger: true });
    if (!ok) return;
    await run(async () => { await api.saveScanOffsets({ revision: data!.revision, mode, angle: a, measured: [] }); await reload(); });
  }

  // The table's chart: offset against speed, one line per angle.
  const chart = $derived.by(() => {
    const entries = data?.entries ?? [];
    if (!entries.length) return null;
    const maxSpeed = Math.max(...entries.map((e) => e.speed)) * 1.1;
    const maxOffset = Math.max(0.1, ...entries.map(e => e.offset)) * 1.15;
    const minOffset = Math.min(0, ...entries.map(e => e.offset)) * 1.15;
    const x = (s: number) => 40 + (s / maxSpeed) * 340, y = (o: number) => 170 - ((o - minOffset) / (maxOffset - minOffset)) * 150;
    const lines = angles.map((a, i) => {
      const points = entries.filter((e) => e.angle === a).sort((p, q) => p.speed - q.speed);
      return { angle: a, color: ['var(--accent)', '#2f8fd8', '#2fa86b', '#c05ad6'][i % 4]!, d: points.map((p, k) => `${k ? 'L' : 'M'}${x(p.speed)} ${y(p.offset)}`).join(''), points: points.map((p) => ({ cx: x(p.speed), cy: y(p.offset), speed: p.speed })) };
    });
    return { lines, ticks: [0, 0.5, 1].map((t) => ({ y: y(minOffset + (maxOffset - minOffset) * t), label: (minOffset + (maxOffset - minOffset) * t).toFixed(2) })), top: fmt(maxSpeed / 1.1, 0) };
  });
</script>

<div class="calibration">
  <header>
    <p>{tr("Per speed: horizontal, 45° and vertical. Measure and save each direction separately.")}</p>
    <div class="seg" aria-label={tr("Calibration laser")}>
      {#each ['fiber', 'co2'] as laser}
        <button class:on={mode === laser} disabled={busy} onclick={() => (mode = laser as LaserMode)}>{tr(laserLabel(laser as LaserMode))}</button>
      {/each}
    </div>
  </header>
  <nav aria-label={tr("Calibration steps")}>
    {#each steps as step, index}
      <button class:current={page === index} disabled={busy} onclick={() => (page = index)}><span>{index + 1}</span>{step}</button>
    {/each}
  </nav>
  {#if error}<p class="error" role="alert">{tr(error)}</p>{/if}
  {#if data && bed}
    <div class="cal-body">
      <div class="bed-wrap">
        <div class="bed-heading"><span>{tr(laserLabel(mode))} · {fmt(bed.max.x - bed.min.x, 0)} × {fmt(bed.max.y - bed.min.y, 0)} mm</span><span>{tr("Back of bed · Y+")}</span></div>
        <svg class="bed" viewBox={bedView} role="button" tabindex="0"
          aria-label={tr("Where the test lies on the bed; click or use the arrow keys to move it")} onclick={place} onkeydown={nudge}>
          <rect class="table" x={bed.min.x} y={bed.min.y} width={bed.max.x - bed.min.x} height={bed.max.y - bed.min.y} />
          <g transform="translate(0 {bed.max.y + bed.min.y}) scale(1 -1)">
            {#each tests as test, i (i)}
              {@const corner = test.corner}
              {@const groupSize = test.size}
              {@const s = test.speed}
              {@const envelope = envelopes[i]!}
              <g class="group" class:measured={savedAt(s, test.angle) !== null} class:now={page === 1 && i === current}>
                {#if !zoomTest}<rect class="overscan" x={envelope.min.x} y={envelope.min.y} width={envelope.max.x - envelope.min.x} height={envelope.max.y - envelope.min.y} />{/if}
                <rect class="frame" x={corner[0] - 2} y={corner[1] - 2} width={groupSize[0] + 4} height={groupSize[1] + 4} />
                {#each blocks(corner, test.angle) as block, b (b)}
                  {#each block.lines as [x1, y1, x2, y2], k (k)}<line x1={x1} y1={y1} x2={x2} y2={y2} />{/each}
                {/each}
              </g>
            {/each}
          </g>
          {#each tests as test, i (i)}
            {@const corner = test.corner}
            {@const groupSize = test.size}
            <text class="speed-label" x={corner[0] + groupSize[0] / 2} y={bed.max.y + bed.min.y - corner[1] - groupSize[1] - 4}
              font-size={3}>{test.angle}°</text>
          {/each}
          {#each tests.filter((_, i) => i % testAngles.length === 0) as test (test.speed)}
            <text class="speed-label" x={test.corner[0] - 5} y={bed.max.y + bed.min.y - test.corner[1] - 5}
              style="text-anchor:end" font-size={3}>{fmt(test.speed, 1)} mm/s</text>
          {/each}
          {#if origin}<circle class="corner" cx={origin[0]} cy={bed.max.y + bed.min.y - origin[1]} r={zoomTest ? 1.5 : (bed.max.x - bed.min.x) / 160} />{/if}
        </svg>
        <div class="bed-heading"><span>{tr("Front of bed")}</span><span>X →</span></div>
        <p class="progress">
          {#if origin}{tr("Test corner X {0} · Y {1} mm · {2} × {3} mm", fmt(origin[0], 0), fmt(origin[1], 0), fmt(testSize[0], 0), fmt(testSize[1], 0))}{#if !fits} · <span class="warn-text">{tr("does not fit there")}</span>{/if}{/if}
          {tr("· {0} of {1} measurements saved", measured, tests.length)}
        </p>
        <p class="progress">{tl("The blocks sit close together. Laser-off run-ups may overlap; run-up reserve and bed limits are still respected.")}</p>
        <div class="corner-buttons">
          <button class="btn btn-ghost" onclick={() => (zoomTest = !zoomTest)}>{zoomTest ? tr("Show full bed") : tr("Enlarge test preview")}</button>
          <button class="btn btn-ghost" disabled={busy} onclick={() => moveOrigin(0)}>{tr("Corner X")}</button>
          <button class="btn btn-ghost" disabled={busy} onclick={() => moveOrigin(1)}>{tr("Corner Y")}</button>
        </div>
      </div>
      <div class="step-content">
        {#if page === 0}
          <span class="eyebrow">{tr("1 · Engrave test")}</span>
          <h3>{tr("{0} speeds · {1} lines", speeds.length, tests.length * LINES)}</h3>
          <p>{threeAngles ? tr("One block per speed: four horizontal, four 45° and four vertical lines. Dashed areas show reserved laser-off travel.") : tr("Four lines per speed at the chosen angle. Dashed areas show reserved laser-off travel.")}</p>
          <div class="speeds" role="group" aria-label={tr("Test speeds")}>
            {#each speeds as s, i (s)}
              <span class="speed"><button class="chip on" disabled={busy} onclick={() => editSpeed(i)}>{fmt(s, 1)} mm/s</button>{#if speeds.length > 1}<button class="x" aria-label={tr("Remove {0} mm/s", s)} disabled={busy} onclick={() => removeSpeed(i)}>×</button>{/if}</span>
            {/each}
            <button class="chip" disabled={busy || speeds.length >= 24} onclick={addSpeed}>{tr("+ Speed")}</button>
          </div>
          <div class="settings">
            <button disabled={busy} onclick={() => { threeAngles = !threeAngles; current = 0; }}><span>{tr("Test directions")}</span><strong>{threeAngles ? '0° / 45° / 90°' : tr("Custom angle")}</strong><small>{tr("Click to switch between three directions and one angle")}</small></button>
            {#if !threeAngles}<button disabled={busy} onclick={() => number(tr('Scan angle'), angle, '°', v => { angle = v; current = 0; }, v => v >= 0 && v < 360, tr('Enter 0 to 359°.'))}><span>{tr("Scan angle")}</span><strong>{fmt(angle, 1)}°</strong><small>{tr("0° along X · 90° along Y")}</small></button>{/if}
            <button disabled={busy} onclick={() => number(tr('Line length'), length, 'mm', (v) => (length = v), (v) => v >= 3 && v <= 50, tr('Enter 3 to 50 mm.'))}><span>{tr("Line length")}</span><strong>{fmt(length, 1)} mm</strong></button>
            <button disabled={busy} onclick={() => number(tr('Line spacing'), spacing, 'mm', (v) => (spacing = v), (v) => v >= 0.3 && v <= 5, tr('Enter 0.3 to 5 mm.'))}><span>{tr("Line spacing")}</span><strong>{fmt(spacing, 2)} mm</strong><small>{tr("Wide enough to tell the lines apart")}</small></button>
          </div>
          <p>{tr("Automatic ramp reserve: machine acceleration {0} mm/s², ramp time {1} ms; 50% extra distance plus 5 mm. One row per speed, 0° / 45° / 90° from left to right.", fmt(data.acceleration, 0), fmt(data.acceleration_time * 1000, 0))}</p>
          <label class="recipe">
            <span>{tr("Recipe for power and frequency")}</span>
            <select bind:value={recipeId} disabled={busy || !recipes.length}>
              {#if !recipes.length}<option value={null}>{tl("No engraving recipe yet")}</option>{/if}
              {#each recipes as r (r.id)}<option value={r.id}>{materialLabel(r.name)} · {tr(r.gas)}</option>{/each}
            </select>
            {#if !recipe}<small>{tl("Create an engraving recipe for this laser in the material library first.")}</small><button class="btn btn-ghost" onclick={() => ui.tab = 'materials'}>{tl("To the material library")}</button>{/if}
          </label>
          <button class="btn btn-ghost" disabled={busy} onclick={() => { if (bed) origin = [bed.min.x + before[0] + 10, bed.min.y + before[1] + 10]; }}>{tr("Place with safe edge clearance")}</button>
          <button class="btn btn-primary lg" disabled={busy || !fits || !recipe} onclick={loadTest}>{tr("Load test in Setup")}</button>
          <button class="text-action" disabled={busy} onclick={() => (page = 1)}>{tr("Already engraved? Measure →")}</button>
        {:else if page === 1}
          <span class="eyebrow">{tr("2 · Measurement {0} of {1}", Math.min(current, tests.length - 1) + 1, tests.length)}</span>
          <h3>{tr("{0} mm/s at {1}°", fmt(speed, 1), fmt(measureAngle, 1))}</h3>
          <svg class="explain" viewBox="0 0 200 70" role="img" aria-label={tr("Lines 1 and 3 run right, 2 and 4 run back: their ends stand apart by the shift")}>
            {#each [0, 1, 2, 3] as k (k)}
              {@const back = k % 2 === 1}
              <line x1={20 + (back ? -6 : 6)} y1={12 + k * 13} x2={150 + (back ? -6 : 6)} y2={12 + k * 13} class:back />
              <text x={back ? 158 : 170} y={15 + k * 13}>{back ? '←' : '→'}</text>
            {/each}
            <line class="dim" x1="14" y1="58" x2="14" y2="66" /><line class="dim" x1="26" y1="58" x2="26" y2="66" />
            <line class="dim" x1="14" y1="63" x2="26" y2="63" />
            <text class="dim-text" x="32" y="66">{tr("shift")}</text>
          </svg>
          <p>{tr('Positive shift: the → line ends further in the → direction. Use a negative value for the opposite shift.')}</p>
          <p>{tr("At a block's edge, the ends of the lines run → stand apart from those run ←. Measure that shift with a loupe or calipers, at either edge.")}</p>
          <button class="value" disabled={busy} onclick={measure}>
            <span>{tr("Shift at {0} mm/s", fmt(speed, 1))}</span>
            <strong>{savedAt(speed) === null ? tr("Enter shift") : `${fmt(savedAt(speed)! * 2, 3)} mm`}</strong>
            {#if savedAt(speed) !== null}<small>{tr("offset {0} mm each way · saved", fmt(savedAt(speed)!, 3))}</small>{/if}
          </button>
          <div class="dots" role="group" aria-label={tr("Speeds")}>
            {#each tests as test, i (i)}
              <button class:done={savedAt(test.speed, test.angle) !== null} class:on={i === current} disabled={busy} onclick={() => (current = i)}>{fmt(test.speed, 1)} · {test.angle}°</button>
            {/each}
          </div>
          <button class="btn btn-primary lg" disabled={busy} onclick={() => { if (current < tests.length - 1) current += 1; else page = 2; }}>
            {current < tests.length - 1 ? tr("Next measurement") : tr("See the table")}
          </button>
        {:else}
          <span class="eyebrow">{tr("3 · Table")}</span>
          <h3>{data.entries.length ? tr("{0} measurements", data.entries.length) : tr("Nothing measured yet")}</h3>
          <p>{tr("Every fill takes the offset for its speed and angle, in between interpolated. An angle on its own counts for every direction.")}</p>
          {#if chart}
            <svg class="chart" viewBox="0 0 400 200" role="img" aria-label={tr("Offset against speed")}>
              {#each chart.ticks as t (t.label)}<line class="grid" x1="40" x2="380" y1={t.y} y2={t.y} /><text class="tick" x="34" y={t.y + 3}>{t.label}</text>{/each}
              <text class="tick" x="380" y="190">{chart.top} mm/s</text>
              {#each chart.lines as line (line.angle)}
                <path d={line.d} stroke={line.color} />
                {#each line.points as p (p.speed)}<circle cx={p.cx} cy={p.cy} r="3" fill={line.color} />{/each}
              {/each}
            </svg>
          {/if}
          {#each angles as a (a)}
            <div class="angle-head"><strong>{fmt(a, 1)}°</strong><button class="link" disabled={busy} onclick={() => clearAngle(a)}>{tr("Delete")}</button></div>
            <dl>
              {#each data.entries.filter((e) => e.angle === a) as e (e.speed)}
                <div><dt>{fmt(e.speed, 1)} mm/s</dt><dd>{tr("{0} mm each way", fmt(e.offset, 3))}</dd></div>
              {/each}
            </dl>
          {/each}
        {/if}
        {#if saved}<p class="saved" role="status">{saved}</p>{/if}
      </div>
    </div>
  {:else if !error}<p>{tr("Loading calibration…")}</p>{/if}
</div>

<style>
  .calibration { max-width: 1180px; margin: 0 auto; padding: 0 10px 20px; }
  header { display:flex; align-items:center; justify-content:space-between; gap:24px; margin-bottom:24px; }
  header p { color:var(--ink-3); margin:0; }
  header .seg button { min-height:48px; }
  nav { display:flex; gap:8px; margin-bottom:24px; }
  nav button { flex:1; display:flex; align-items:center; gap:12px; padding:12px 18px; min-height:58px; border:1px solid var(--line); border-radius:10px; color:var(--ink-3); background:var(--panel-2); text-align:left; font-size:var(--t-base); cursor:pointer; }
  nav button span { display:grid; place-items:center; width:27px; height:27px; border:1px solid var(--line); border-radius:50%; }
  nav button.current { color:var(--ink); border-color:var(--accent); background:color-mix(in srgb,var(--accent) 8%,var(--panel)); }
  nav .current span { color:white; background:var(--accent); border-color:var(--accent); }
  .cal-body { display:grid; grid-template-columns:minmax(0,1.3fr) minmax(300px,1fr); gap:34px; align-items:start; }
  .bed { width:100%; height:auto; max-height:52vh; border:2px solid var(--line); border-radius:12px; background:var(--panel-2); cursor:crosshair; display:block; }
  .table { fill:var(--panel); stroke:var(--line); stroke-width:2; vector-effect:non-scaling-stroke; }
  .group .overscan { fill:none; stroke:var(--ink-3); stroke-width:1; stroke-dasharray:5 4; vector-effect:non-scaling-stroke; }
  .group .frame { fill:color-mix(in srgb,var(--accent) 14%,transparent); stroke:var(--accent); stroke-width:1; vector-effect:non-scaling-stroke; }
  .group line { stroke:var(--ink); stroke-width:1; vector-effect:non-scaling-stroke; }
  .group.measured .frame { fill:color-mix(in srgb,var(--ok) 22%,transparent); stroke:var(--ok); }
  .group.now .frame { stroke-width:3; }
  .speed-label { stroke:none; fill:var(--ink-2); text-anchor:middle; font-weight:700; pointer-events:none; }
  .corner { fill:var(--accent); }
  .bed-heading { display:flex; justify-content:space-between; gap:15px; color:var(--ink-3); font-size:var(--t-sm); padding:8px 0; }
  .progress { color:var(--ink-3); font-size:var(--t-sm); margin:6px 0; }
  .corner-buttons { display:flex; gap:8px; flex-wrap:wrap; }
  .eyebrow { text-transform:uppercase; letter-spacing:.12em; font-size:var(--t-sm); color:var(--accent); font-weight:700; }
  h3 { font-size:var(--t-xl); line-height:1.2; margin:10px 0 14px; color:var(--ink); text-transform:none; letter-spacing:normal; }
  .step-content p { color:var(--ink-3); font-size:var(--t-sm); line-height:1.6; margin:0 0 14px; }
  .step-content > .btn { width:100%; min-height:56px; margin-top:10px; }
  .text-action { min-height:50px; width:100%; border:0; background:none; color:var(--ink-2); font-size:var(--t-sm); cursor:pointer; margin-top:6px; }
  .speeds { display:flex; flex-wrap:wrap; gap:8px; margin:0 0 14px; }
  .speed { display:inline-flex; align-items:center; }
  .speed .x { width:36px; min-height:44px; border:0; background:none; color:var(--ink-3); font-size:var(--t-lg); cursor:pointer; }
  .speeds .chip { min-height:44px; }
  .settings { display:grid; grid-template-columns:repeat(auto-fill,minmax(130px,1fr)); gap:10px; margin:0 0 14px; }
  .settings button, .value { min-height:84px; text-align:left; padding:12px; border:1px solid var(--line); border-radius:10px; background:var(--panel-2); color:var(--ink); cursor:pointer; display:grid; gap:4px; align-content:start; }
  .value { width:100%; min-height:96px; border-color:var(--accent); }
  .settings span, .value span { font-size:var(--t-sm); color:var(--ink-3); }
  .settings strong { font-size:var(--t-lg); overflow-wrap:anywhere; }
  .settings button, .step-content, .bed-wrap { min-width:0; }
  .recipe select { width:100%; min-width:0; }
  .dots button { padding:8px 12px; }
  .value strong { font-size:var(--t-xl); }
  .settings small, .value small { font-size:var(--t-sm); color:var(--ink-3); }
  .recipe { display:grid; gap:6px; margin:0 0 6px; font-size:var(--t-sm); color:var(--ink-3); }
  .recipe select { min-height:48px; border-radius:10px; border:1px solid var(--line); background:var(--panel-2); color:var(--ink); padding:0 10px; font-size:var(--t-base); }
  .explain { width:100%; max-width:360px; height:auto; margin:0 0 10px; }
  .explain line { stroke:var(--accent); stroke-width:3; stroke-linecap:round; }
  .explain line.back { stroke:#2f8fd8; }
  .explain text { fill:var(--ink-2); font-size:10px; }
  .explain .dim { stroke:var(--ink-3); stroke-width:1; }
  .explain .dim-text { fill:var(--ink-3); font-size:8px; }
  .dots { display:flex; flex-wrap:wrap; gap:6px; margin:12px 0 0; }
  .dots button { min-width:52px; min-height:40px; border-radius:20px; border:1px solid var(--line); background:var(--panel-2); color:var(--ink-2); cursor:pointer; font-size:var(--t-sm); }
  .dots button.done { border-color:var(--ok); color:var(--ok); }
  .dots button.on { outline:2px solid var(--accent); outline-offset:2px; }
  .chart { width:100%; height:auto; margin:0 0 12px; }
  .chart path { fill:none; stroke-width:2.5; }
  .chart .grid { stroke:var(--line); }
  .chart .tick { fill:var(--ink-3); font-size:10px; text-anchor:end; }
  .angle-head { display:flex; justify-content:space-between; align-items:center; margin-top:12px; }
  dl { margin:6px 0 0; }
  dl div { display:flex; justify-content:space-between; gap:15px; padding:9px 0; border-bottom:1px solid var(--line); font-size:var(--t-sm); }
  dt { color:var(--ink-3); }
  dd { margin:0; }
  .error { color:var(--warn); }
  .saved { color:var(--ok); margin-top:14px; }
  @media(max-width:820px) { .cal-body { grid-template-columns:1fr; } header { flex-direction:column; align-items:flex-start; } }
</style>
