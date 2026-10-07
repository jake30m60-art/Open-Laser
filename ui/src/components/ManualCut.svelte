<script lang="ts">
  import { t } from '../i18n/index';
  import Modal from './Modal.svelte';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { osk } from '../lib/osk.svelte';
  import { frameOf } from '../lib/frame';
  import { isEngraving } from '../lib/engraving';
  import { explain } from '../lib/format';
  let { onclose }: { onclose: () => void } = $props();
  const initialDoc = server.doc!;
  const initialBed = frameOf(initialDoc).bed;
  const doc = $derived(server.doc!);
  const bed = $derived(frameOf(doc).bed);
  const recipes = $derived(doc.library.recipes.filter(r => r.laser === doc.mode && !isEngraving(r)));
  let recipeId = $state(initialDoc.draft?.recipe && !isEngraving(initialDoc.draft.recipe) ? initialDoc.draft.recipe.id : '');
  const recipe = $derived(recipes.find(r => r.id === recipeId) ?? null);
  let start = $state<[number, number]>([initialBed ? initialBed.minX + 10 : 0, initialBed ? (initialBed.minY + initialBed.maxY) / 2 : 0]);
  let end = $state<[number, number]>([initialBed ? initialBed.maxX - 10 : 100, initialBed ? (initialBed.minY + initialBed.maxY) / 2 : 0]);
  let picking = $state<'start' | 'end'>('start');
  let busy = $state(false), error = $state('');
  const length = $derived(Math.hypot(end[0] - start[0], end[1] - start[1]));
  const fits = $derived(!!bed && [start, end].every(p => p.every(Number.isFinite) && p[0] >= bed.minX && p[0] <= bed.maxX && p[1] >= bed.minY && p[1] <= bed.maxY) && length >= 0.01);
  const fmt = (n: number) => Number(n.toFixed(2)).toString();
  function preset(axis: 0 | 1): void {
    if (!bed) return;
    const centre: [number, number] = [(bed.minX + bed.maxX) / 2, (bed.minY + bed.maxY) / 2];
    start = [...centre]; end = [...centre];
    start[axis] = axis === 0 ? bed.minX + 10 : bed.minY + 10;
    end[axis] = axis === 0 ? bed.maxX - 10 : bed.maxY - 10;
  }
  function coordinate(which: 'start' | 'end', axis: 0 | 1): void {
    const p = which === 'start' ? start : end;
    osk.number(`${which === 'start' ? 'Start' : 'Ende'} ${axis === 0 ? 'X' : 'Y'}`, p[axis], 'mm', v => {
      if (!Number.isFinite(v)) { error = 'Bitte einen gültigen Koordinatenwert eingeben.'; return; }
      const next: [number, number] = [...p]; next[axis] = v;
      if (which === 'start') start = next; else end = next;
    });
  }
  function place(event: MouseEvent): void {
    if (!bed || busy) return;
    const svg = event.currentTarget as SVGSVGElement, p = svg.createSVGPoint();
    p.x = event.clientX; p.y = event.clientY;
    const at = p.matrixTransform(svg.getScreenCTM()!.inverse());
    const next: [number, number] = [Math.round(at.x * 100) / 100, Math.round((bed.minY + bed.maxY - at.y) * 100) / 100];
    if (picking === 'start') { start = next; picking = 'end'; } else end = next;
  }
  function atHead(): void {
    const head = frameOf(doc).head;
    if (head) { start = head; picking = 'end'; }
  }
  async function load(): Promise<void> {
    if (!recipe || !fits || busy) return;
    busy = true; error = '';
    try {
      await api.manualCut({ recipe: recipe.id, start: $state.snapshot(start), end: $state.snapshot(end) });
      ui.setupPanel = null; ui.tab = 'setup';
      ui.say('Trennschnitt in Vorbereitung geladen. Schnittpfad prüfen und anschließend über Ausführen starten.');
      onclose();
    } catch (e) { error = explain(e); } finally { busy = false; }
  }
</script>

<Modal title={t("Manueller Trennschnitt")} wide onclose={onclose} dismissable={!busy}>
  <p>{t("Start und Ende auf dem Maschinentisch festlegen. Der Schnitt wird als eigener Auftrag vorbereitet; Vorschau, Trockenlauf und die üblichen Startkontrollen bleiben verfügbar.")}</p>
  {#if bed}
    <div class="cut-grid">
      <div>
        <svg class="cut-bed" viewBox="{bed.minX - 20} {bed.minY - 20} {bed.maxX - bed.minX + 40} {bed.maxY - bed.minY + 40}" onclick={place} role="button" tabindex="0" aria-label={t("Trennschnitt platzieren; alternativ Koordinatenfelder verwenden")} onkeydown={e => { if (e.key === 'Escape') onclose(); }}>
          <rect x={bed.minX} y={bed.minY} width={bed.maxX-bed.minX} height={bed.maxY-bed.minY} />
          <g transform="translate(0 {bed.minY+bed.maxY}) scale(1 -1)">
            <line x1={start[0]} y1={start[1]} x2={end[0]} y2={end[1]} />
            <circle class="start" cx={start[0]} cy={start[1]} r={(bed.maxX-bed.minX)/90} />
            <circle class="end" cx={end[0]} cy={end[1]} r={(bed.maxX-bed.minX)/90} />
          </g>
        </svg>
        <p>Grün: Start · Orange: Ende · Länge {fmt(length)} mm</p>
        <div class="buttons"><button class="btn btn-ghost" onclick={() => preset(0)}>{t("Horizontal")}</button><button class="btn btn-ghost" onclick={() => preset(1)}>{t("Vertikal")}</button><button class="btn btn-ghost" onclick={() => { const p=start; start=end; end=p; }}>{t("Richtung umkehren")}</button></div>
      </div>
      <div class="cut-settings">
        <div class="seg"><button class:on={picking==='start'} onclick={() => picking='start'}>{t("Start auf Tisch wählen")}</button><button class:on={picking==='end'} onclick={() => picking='end'}>{t("Ende auf Tisch wählen")}</button></div>
        {#each ['start','end'] as which}
          <h3>{which === 'start' ? 'Start' : 'Ende'} · Maschinenkoordinaten</h3>
          <div class="buttons">{#each [0,1] as axis}<button class="btn btn-soft" onclick={() => coordinate(which as 'start' | 'end', axis as 0 | 1)}>{axis === 0 ? 'X' : 'Y'} {fmt((which === 'start' ? start : end)[axis]!)} mm</button>{/each}</div>
        {/each}
        {#if frameOf(doc).head}<button class="btn btn-ghost" onclick={atHead}>{t("Aktuelle Kopfposition als Start")}</button>{/if}
        <label>{t("Schneidrezept")}<select bind:value={recipeId}><option value="">{t("Rezept auswählen…")}</option>{#each recipes as r (r.id)}<option value={r.id}>{r.name} · {r.thickness_mm} mm · {r.gas}</option>{/each}</select></label>
        {#if recipe}<p>{recipe.summary.speed ?? '—'} mm/s · Leistung {recipe.summary.duty ?? '—'} % · Gas {recipe.gas}</p>{/if}
        {#if !fits}<p class="warn-text">{t("Beide Punkte müssen innerhalb des Verfahrbereichs liegen und mindestens 0,01 mm voneinander entfernt sein.")}</p>{/if}
        {#if error}<p class="warn-text" role="alert">{error}</p>{/if}
        <button class="btn btn-primary lg" disabled={!recipe || !fits || busy} onclick={load}>{busy ? 'Wird vorbereitet…' : 'Trennschnitt in Vorbereitung laden'}</button>
      </div>
    </div>
  {:else}<p class="warn-text">{t("Maschinensicherung laden, damit der Verfahrbereich bekannt ist.")}</p>{/if}
</Modal>

<style>
  .cut-grid { display:grid; grid-template-columns:minmax(0,1.2fr) minmax(0,1fr); gap:24px; }
  .cut-bed { display:block; width:100%; max-height:55vh; background:var(--panel-2); border:1px solid var(--line); border-radius:12px; cursor:crosshair; }
  rect { fill:var(--panel); stroke:var(--line); stroke-width:2; vector-effect:non-scaling-stroke; }
  line { stroke:var(--accent); stroke-width:3; vector-effect:non-scaling-stroke; }
  .start { fill:var(--ok); } .end { fill:var(--accent); }
  .buttons { display:flex; gap:8px; flex-wrap:wrap; }
  .cut-settings { min-width:0; display:flex; flex-direction:column; gap:12px; }
  h3 { margin:0; } p { line-height:1.5; }
  label { display:grid; gap:8px; } select { min-width:0; width:100%; min-height:48px; }
  .seg { flex-wrap:wrap; } button { white-space:normal; height:auto; line-height:1.3; }
  @media (max-width:850px) { .cut-grid { grid-template-columns:minmax(0,1fr); } }
</style>
