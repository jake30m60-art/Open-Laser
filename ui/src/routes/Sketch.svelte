<script lang="ts">
  import CreateText from '../components/CreateText.svelte';
  import { appendDrawing } from '../sketch/append-drawing';
  import { beginProject } from '../lib/sketch-workflow';
  import { t } from '../i18n/index';

  // A 2D sketch drawn as in Fusion: create, modify, constrain and dimension.
  // Draw with a tool, type a value to lock it (Tab moves on, Enter places);
  // constraints and dimensions are solved by FreeCAD's PlaneGCS, blue
  // geometry can still move and black is fully defined. Escape steps back.
  // The finished sketch becomes a library part through the DXF importer.
  import { onMount } from 'svelte';
  import wasmUrl from '@salusoft89/planegcs/dist/planegcs_dist/planegcs.wasm?url';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { osk } from '../lib/osk.svelte';
  import { explain } from '../lib/format';
  import { displayNumber, toSource, unitLabel } from '../lib/units.svelte';
  import { sketchStore as store } from '../sketch/store.svelte';
  import { clone, isDimension, merge, placedText, point, scaledText, type Constraint, type Dimension, type Id, type Sketch, type SketchText } from '../sketch/model';
  import { angleOf, dist, polar, shapeOf, sketchBounds, sweep, insideBox, touchesBox, type Shape, type Vec } from '../sketch/geometry';
  import { gridStep, snap, type Snap } from '../sketch/snap';
  import { type Field, type Locked, type ToolId } from '../sketch/tools';
  import { type ActionId, type Hit, type Step } from '../sketch/actions';
  import { chain, hit, hitPoint, originsOf, repairArcs, translate } from '../sketch/edit';
  import { initSolver, solve, solverReady, defined, type Pull } from '../sketch/solver';
  import { draw as drawDimension, glyphs, label as dimensionLabel, measure } from '../sketch/annotations';
  import { hasCut, toDxf } from '../sketch/dxf';
  import { evaluate } from '../sketch/expression';

  let textOpen=$state(false), importing=$state(false);
  /** The sketch text being changed in the text dialog, if any. */
  let editingText=$state<SketchText|null>(null);
  /** The text whose outline box holds `m`, topmost first. */
  function textAt(s: Sketch, m: Vec): Id | null {
    const pad = reach;
    const found = [...(s.texts ?? [])].reverse().find((x) => m.x >= x.x - pad && m.x <= x.x + x.width + pad && m.y >= x.y - pad && m.y <= x.y + x.height + pad);
    return found?.id ?? null;
  }
  function editText(id: Id): void {
    editingText = store.sketch.texts?.find((x) => x.id === id) ?? null;
    if (editingText) textOpen = true;
  }
  /** A new text goes where the view is centred; a changed one keeps its corner. */
  function placeText(text: Omit<SketchText, 'id' | 'x' | 'y'>): void {
    const changing = editingText;
    const error = store.change((s) => {
      s.texts ??= [];
      if (changing) {
        const at = s.texts.findIndex((x) => x.id === changing.id);
        if (at >= 0) s.texts[at] = { ...text, id: changing.id, x: changing.x, y: changing.y };
        return;
      }
      const id = s.next++;
      s.texts.push({ ...text, id, x: view.cx - text.width / 2, y: view.cy - text.height / 2 });
      store.select([id]);
    });
    if (error) ui.say(error, true);
    editingText = null;
  }
  const textPath = (text: SketchText): string => placedText(text).map((line) => line.map(([x, y], i) => { const q = toScreen({ x, y }); return `${i ? 'L' : 'M'}${q.x.toFixed(1)} ${q.y.toFixed(1)}`; }).join('')).join('');
  let importInput:HTMLInputElement;
  async function insertPart(id:string){const {drawing}=await api.partDrawing(id);let ids:number[]=[];const error=store.change(s=>{ids=appendDrawing(s,drawing);});if(error)throw new Error(error);store.select(ids);fit();}
  async function importDrawing(event:Event){const input=event.currentTarget as HTMLInputElement,file=input.files?.[0];if(!file)return;importing=true;try{const {id,warnings}=await api.importPart(file.name,await file.arrayBuffer());await insertPart(id);if(warnings?.length)ui.say(warnings.join(' · '));}catch(e){ui.say(explain(e),true);}finally{importing=false;input.value='';}}

  type Button<T> = { id: T; label: string; key?: string; icon?: string; glyph?: string };
  const CREATE: Button<ToolId | null>[] = [
    { id: null, label: t("Select"), key: 'Esc', icon: 'M6 3l12 9-5.5 1.2L16 20l-2.6 1.2-2.9-6.7L6 18z' },
    { id: 'line', label: t("Line"), key: 'L', icon: 'M5 19L19 5M5 19m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0M19 5m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0' },
    { id: 'rect2', label: t("2-point rectangle"), key: 'R', icon: 'M4 6h16v12H4zM4 18m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0M20 6m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0' },
    { id: 'rectc', label: t("Center rectangle"), icon: 'M4 6h16v12H4zM4 6l16 12M20 6L4 18' },
    { id: 'circle', label: t("Center diameter circle"), key: 'C', icon: 'M12 4a8 8 0 1 0 0 16a8 8 0 1 0 0-16M12 12h8M12 12m-1.2 0a1.2 1.2 0 1 0 2.4 0a1.2 1.2 0 1 0-2.4 0' },
    { id: 'circle2', label: t("2-point circle"), icon: 'M12 4a8 8 0 1 0 0 16a8 8 0 1 0 0-16M4 12h16' },
    { id: 'arc3', label: t("3-point arc"), key: 'A', icon: 'M4 18A9 9 0 0 1 20 18M4 18m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0M20 18m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0M12 9m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0' },
    { id: 'arcc', label: t("Center point arc"), icon: 'M20 15A8 8 0 0 0 7 8.5M12 15h8M12 15L7 8.5M12 15m-1.2 0a1.2 1.2 0 1 0 2.4 0a1.2 1.2 0 1 0-2.4 0' },
    { id: 'polygon', label: t("Inscribed polygon"), key: 'P', icon: 'M12 3l7.8 4.5v9L12 21l-7.8-4.5v-9z' },
    { id: 'slot', label: t("Center to center slot"), icon: 'M8 7h8a5 5 0 0 1 0 10H8A5 5 0 0 1 8 7M8 12h8' },
  ];
  const MODIFY: Button<ActionId>[] = [
    { id: 'fillet', label: t("Fillet"), icon: 'M4 20V12a8 8 0 0 1 8-8h8' },
    { id: 'chamfer', label: t("Chamfer"), icon: 'M4 20v-9l7-7h9' },
    { id: 'trim', label: t("Trim"), key: 'T', icon: 'M3 12h7M14 12h7M12 3v18M10 9l4 6M14 9l-4 6' },
    { id: 'extend', label: t("Extend"), icon: 'M3 12h12M15 9l4 3-4 3M20 4v16' },
    { id: 'break', label: t("Break"), icon: 'M3 12h7M14 12h7M12 5v14' },
    { id: 'offset', label: t("Offset"), key: 'O', icon: 'M5 19V9a4 4 0 0 1 4-4h10M9 19v-8a2 2 0 0 1 2-2h8' },
    { id: 'mirror', label: t("Mirror"), icon: 'M12 3v18M9 7L4 17h5zM15 7l5 10h-5z' },
    { id: 'rpattern', label: t("Rectangular pattern"), icon: 'M4 4h5v5H4zM15 4h5v5h-5zM4 15h5v5H4zM15 15h5v5h-5z' },
    { id: 'cpattern', label: t("Circular pattern"), icon: 'M12 3a2 2 0 1 0 0 4a2 2 0 1 0 0-4M19 10a2 2 0 1 0 0 4a2 2 0 1 0 0-4M12 17a2 2 0 1 0 0 4a2 2 0 1 0 0-4M5 10a2 2 0 1 0 0 4a2 2 0 1 0 0-4' },
  ];
  const RELATE: Button<ActionId>[] = [
    { id: 'coincident', label: t("Coincident"), glyph: '•' },
    { id: 'collinear', label: t("Collinear"), glyph: '≡' },
    { id: 'concentric', label: t("Concentric"), glyph: '◎' },
    { id: 'midpoint', label: t("Midpoint"), glyph: 'Δ' },
    { id: 'fix', label: t("Fix / unfix"), glyph: '⚓' },
    { id: 'parallel', label: t("Parallel"), glyph: '∥' },
    { id: 'perpendicular', label: t("Perpendicular"), glyph: '⊥' },
    { id: 'hv', label: t("Horizontal / vertical"), glyph: 'H|V' },
    { id: 'tangent', label: t("Tangent"), glyph: 'T' },
    { id: 'equal', label: t("Equal"), glyph: '=' },
    { id: 'symmetric', label: t("Symmetric"), glyph: '⇔' },
  ];
  const TOOL_KEYS: Record<string, ToolId> = { l: 'line', r: 'rect2', c: 'circle', a: 'arc3', p: 'polygon' };
  const ACTION_KEYS: Record<string, ActionId> = { d: 'dimension', t: 'trim', o: 'offset' };

  // ---- View: millimetres with Y up, drawn in screen pixels -------------------------------------
  let w = $state(800);
  let h = $state(600);
  const needsInitialFit = store.view === null;
  let view = $state(store.view ?? { cx: 100, cy: 60, s: 3 });
  $effect(() => { store.view = view; });
  let svg: SVGSVGElement | undefined = $state();
  const toScreen = (p: Vec): Vec => ({ x: (p.x - view.cx) * view.s + w / 2, y: h / 2 - (p.y - view.cy) * view.s });
  const toModel = (x: number, y: number): Vec => ({ x: view.cx + (x - w / 2) / view.s, y: view.cy - (y - h / 2) / view.s });

  // ---- Cursor, snapping and the heads-up fields ----------------------------------------------------
  let cursor = $state.raw<Vec | null>(null);
  let hover = $state.raw<Hit | null>(null);
  let touch = $state(false);
  let locked = $state<Locked>({});
  let editing = $state<{ key: string; text: string } | null>(null);
  let active = $state(0);
  let dragSnap = $state.raw<Snap | null>(null);

  const reach = $derived((touch ? 16 : 9) / view.s);
  /** The grid: the fixed step when one is set, else one that suits the zoom. */
  const minor = $derived(store.gridStep ?? gridStep(1 / view.s, 10));
  const drawnStep = $derived.by(() => { let s = minor; while (s * view.s < 6) s *= 5; return s; });
  const major = $derived(drawnStep * 5);
  const tool = $derived(store.tool);
  const action = $derived(store.action);
  /** What the modify tools work on: the selected curves, not constraints or dimensions. */
  const ctx = $derived({
    sketch: store.sketch,
    selection: new Set([...store.selection].filter((id) => store.sketch.entities.some((e) => e.id === id))) as ReadonlySet<Id>,
  });
  const snapped = $derived.by((): Snap | null => {
    void store.toolRevision;
    if (!cursor || !tool) return null;
    return snap(store.sketch, cursor, { reach, grid: store.gridSnap ? minor : 0, anchor: tool.anchor(), tangent: store.automaticConstraints && tool.id === 'line' });
  });
  const resolved = $derived.by((): Snap | null => { void store.toolRevision; return tool && snapped ? tool.resolve(snapped, locked) : null; });
  const fields = $derived.by((): Field[] => {
    void store.toolRevision;
    if (tool) return resolved ? tool.fields(resolved) : [];
    if (action && cursor) return action.fields(ctx, cursor);
    return [];
  });
  const ghost = $derived.by(() => { void store.toolRevision; return tool && resolved ? tool.ghost(resolved, locked) : null; });
  const preview = $derived.by(() => {
    void store.toolRevision;
    if (!action || !cursor) return null;
    try { return action.preview(ctx, cursor, hover ?? { point: null, entity: null, at: cursor }, locked); } catch { return null; }
  });
  const anchor = $derived.by(() => { void store.toolRevision; return tool?.anchor() ?? null; });
  const picked = $derived.by(() => { void store.toolRevision; return action ? action.picked() : { points: [], curves: [] }; });
  const hint = $derived.by(() => {
    void store.toolRevision;
    if (tool) return tool.hint();
    if (action) return action.hint(ctx);
    return 'Click to select · drag a box · double click a profile or a dimension to edit it';
  });

  // ---- Rendering -------------------------------------------------------------------------------
  function pathOf(shape: Shape): string {
    if (shape.kind === 'line') {
      const a = toScreen(shape.a), b = toScreen(shape.b);
      return `M${a.x} ${a.y}L${b.x} ${b.y}`;
    }
    if (shape.kind === 'circle') {
      const c = toScreen(shape.c), r = shape.r * view.s;
      return `M${c.x - r} ${c.y}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;
    }
    const sw = sweep(shape.start, shape.end);
    const s = toScreen(polar(shape.c, shape.r, shape.start)), e = toScreen(polar(shape.c, shape.r, shape.end)), r = shape.r * view.s;
    if (sw > 2 * Math.PI - 1e-6) return pathOf({ kind: 'circle', c: shape.c, r: shape.r });
    return `M${s.x} ${s.y}A${r} ${r} 0 ${sw > Math.PI ? 1 : 0} 0 ${e.x} ${e.y}`;
  }

  const gridLines = $derived.by(() => {
    const lines: Array<{ d: string; major: boolean }> = [];
    const tl = toModel(0, 0), br = toModel(w, h);
    const step = drawnStep;
    if ((br.x - tl.x) / step > 400) return lines;
    const isMajor = (v: number) => Math.abs(v / major - Math.round(v / major)) < 1e-6;
    for (let x = Math.ceil(tl.x / step) * step; x <= br.x; x += step) lines.push({ d: `M${toScreen({ x, y: 0 }).x} 0V${h}`, major: isMajor(x) });
    for (let y = Math.ceil(br.y / step) * step; y <= tl.y; y += step) lines.push({ d: `M0 ${toScreen({ x: 0, y }).y}H${w}`, major: isMajor(y) });
    return lines;
  });
  const origin = $derived(toScreen({ x: 0, y: 0 }));

  const free = $derived(store.free);
  const drawn = $derived(store.sketch.entities.map((e) => ({
    id: e.id, construction: !!e.construction, d: pathOf(shapeOf(store.sketch, e)),
    defined: !!free && defined(e, free),
  })));
  const centers = $derived(new Set(store.sketch.entities.flatMap((e) => (e.kind === 'line' ? [] : [e.c]))));
  const fixed = $derived(new Set(store.sketch.constraints.flatMap((c) => (c.kind === 'fix' ? [c.point] : []))));
  const handles = $derived(store.sketch.points.map((p) => ({ id: p.id, center: centers.has(p.id), defined: !!free && !free.points.has(p.id), fixed: fixed.has(p.id), ...toScreen(p) })));

  const mm = (v: number) => displayNumber(v, 'mm').replace(/\.?0+$/, '');
  function arrowHead(at: Vec, toward: Vec): string {
    const a = toScreen(at), b = toScreen(toward);
    const L = Math.hypot(b.x - a.x, b.y - a.y) || 1;
    const u = { x: (b.x - a.x) / L, y: (b.y - a.y) / L }, n = { x: -u.y, y: u.x };
    return `M${a.x} ${a.y}L${a.x + u.x * 9 + n.x * 3.5} ${a.y + u.y * 9 + n.y * 3.5}L${a.x + u.x * 9 - n.x * 3.5} ${a.y + u.y * 9 - n.y * 3.5}Z`;
  }
  type DrawnDim = { id: Id; d: string; arrows: string[]; text: Vec; label: string };
  function render(s: Sketch, dim: Dimension): DrawnDim | null {
    const g = drawDimension(s, dim);
    if (!g) return null;
    let d = g.lines.map(([a, b]) => { const p = toScreen(a), q = toScreen(b); return `M${p.x} ${p.y}L${q.x} ${q.y}`; }).join('');
    if (g.arc) d += pathOf({ kind: 'arc', c: g.arc.c, r: g.arc.r, start: g.arc.start, end: g.arc.end });
    return { id: dim.id, d, arrows: g.arrows.map((a) => arrowHead(a.at, a.toward)), text: toScreen(g.text), label: dimensionLabel(dim.driven ? { ...dim, value: measure(s, dim) } : dim, mm) };
  }
  const dims = $derived(store.showDimensions ? store.sketch.constraints.filter(isDimension).map((c) => render(store.sketch, c)).filter((x): x is DrawnDim => !!x) : []);
  const pendingDim = $derived(preview?.dimension ? render(store.sketch, { ...preview.dimension, value: locked.value ?? preview.dimension.value }) : null);
  const marks = $derived.by(() => {
    if (!store.showConstraints) return [];
    const seen = new Map<string, number>();
    return glyphs(store.sketch).map((g) => {
      const q = toScreen(g.at);
      const key = `${Math.round(q.x / 6)},${Math.round(q.y / 6)}`;
      const n = seen.get(key) ?? 0;
      seen.set(key, n + 1);
      return { ...g, x: q.x + 10 + n * 18, y: q.y - 22 };
    });
  });

  // ---- Fitting the view ----------------------------------------------------------------------
  function fit(): void {
    const b = sketchBounds(store.sketch);
    if (!b) { view = { cx: 100, cy: 60, s: Math.min(w / 260, h / 160) }; return; }
    const bw = Math.max(b.maxX - b.minX, 10), bh = Math.max(b.maxY - b.minY, 10);
    view = { cx: (b.minX + b.maxX) / 2, cy: (b.minY + b.maxY) / 2, s: Math.min(w / (bw * 1.25), h / (bh * 1.25)) };
  }
  onMount(() => {
    const frame = requestAnimationFrame(() => { if (needsInitialFit || w < 600) fit(); });
    initSolver(wasmUrl)
      .then(() => { store.solverRevision++; store.refreshFreedom(); })
      .catch((e) => ui.say(t("The constraint solver did not load: {0}", explain(e)), true));
    return () => cancelAnimationFrame(frame);
  });

  // ---- Pointer input ---------------------------------------------------------------------------
  type Drag =
    | { kind: 'pan'; x: number; y: number; cx: number; cy: number }
    | { kind: 'press'; x: number; y: number; start: Vec; moved: boolean; mode: 'tool' | 'action' | 'point' | 'move' | 'radius' | 'box' | 'label' | 'text' | 'text-scale'; target: Id | null; original: Sketch; origins: Map<Id, Vec>; shift: boolean }
    | { kind: 'pinch'; d: number; mid: Vec; view: { cx: number; cy: number; s: number }; model: Vec };
  let drag: Drag | null = null;
  /** The last click on a dimension's value, for telling a double click:
   *  the canvas holds the pointer, so the browser's own never reaches it. */
  let lastLabel: { id: Id; at: number } | null = null;
  let box = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const pointers = new Map<number, { x: number; y: number }>();

  function local(e: PointerEvent | WheelEvent | MouseEvent): { x: number; y: number } {
    const r = svg!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  function hitAt(m: Vec): Hit {
    return { point: hitPoint(store.sketch, m, reach), entity: hit(store.sketch, m, reach), at: m };
  }

  /** The constraint or dimension whose glyph or text is under the pointer. */
  function annotationAt(e: Event): Id | null {
    const el = (e.target as Element | null)?.closest('[data-constraint]');
    return el ? Number(el.getAttribute('data-constraint')) : null;
  }

  function pinchStart(): void {
    const [p, q] = [...pointers.values()];
    if (!p || !q) return;
    const mid = { x: (p.x + q.x) / 2, y: (p.y + q.y) / 2 };
    drag = { kind: 'pinch', d: Math.hypot(p.x - q.x, p.y - q.y) || 1, mid, view: { ...view }, model: toModel(mid.x, mid.y) };
    box = null;
  }

  function down(e: PointerEvent): void {
    const at = local(e);
    touch = e.pointerType === 'touch';
    svg?.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, at);
    if (pointers.size === 2) { if (drag?.kind === 'press' && drag.moved) store.preview(drag.original); pinchStart(); return; }
    if (e.button === 1 || (e.button === 0 && spaceHeld)) { e.preventDefault(); drag = { kind: 'pan', x: at.x, y: at.y, cx: view.cx, cy: view.cy }; return; }
    if (e.button !== 0) return;
    const m = toModel(at.x, at.y);
    cursor = m;
    const original = store.sketch;
    const base = { kind: 'press' as const, x: at.x, y: at.y, start: m, moved: false, original, origins: new Map<Id, Vec>(), shift: e.shiftKey || e.ctrlKey || e.metaKey };
    // A dimension's value answers a click or a double click whatever tool
    // is open, as in Fusion, so it can be changed at any time.
    const note = annotationAt(e);
    const dimension = note !== null && store.sketch.constraints.some((c) => c.id === note && isDimension(c));
    if (dimension) { drag = { ...base, mode: 'label', target: note }; return; }
    if (tool) { drag = { ...base, mode: 'tool', target: null }; return; }
    if (action && !(dimension && !action.busy)) { drag = { ...base, mode: 'action', target: null }; return; }
    if (note !== null) { drag = { ...base, mode: 'label', target: note }; return; }
    const scaling = scaleHandle();
    if (scaling && Math.hypot(m.x - scaling.x, m.y - scaling.y) <= reach * 1.5) { drag = { ...base, mode: 'text-scale', target: scaling.id }; return; }
    const p = hitPoint(original, m, reach);
    if (p !== null) { drag = { ...base, mode: 'point', target: p, origins: originsOf(original, []).set(p, { ...point(original, p) }) }; return; }
    const id = hit(original, m, reach);
    const text = id === null ? textAt(original, m) : null;
    if (text !== null) { drag = { ...base, mode: 'text', target: text }; return; }
    if (id === null) { drag = { ...base, mode: 'box', target: null }; return; }
    const entity = original.entities.find((x) => x.id === id)!;
    if (entity.kind === 'circle' && !store.selection.has(id)) { drag = { ...base, mode: 'radius', target: id }; return; }
    const moving = store.selection.has(id) ? [...store.selection].filter((x) => original.entities.some((en) => en.id === x)) : [id];
    drag = { ...base, mode: 'move', target: id, origins: originsOf(original, moving) };
  }

  /** The sketch with its points pulled, solved when the solver is there. */
  function dragged(original: Sketch, pulls: Pull[], radii: Map<Id, number>): Sketch | null {
    if (solverReady()) {
      const solved = solve(original, pulls, radii);
      return solved.ok ? solved.sketch : null;
    }
    const next = clone(original);
    for (const pull of pulls) { const p = point(next, pull.point); p.x = pull.x; p.y = pull.y; }
    radii.forEach((r, id) => { const c = next.entities.find((x) => x.id === id); if (c?.kind === 'circle') c.r = r; });
    repairArcs(next, original);
    return next;
  }

  function move(e: PointerEvent): void {
    const at = local(e);
    touch = e.pointerType === 'touch';
    if (pointers.has(e.pointerId)) pointers.set(e.pointerId, at);
    if (drag?.kind === 'pinch') {
      const [p, q] = [...pointers.values()];
      if (!p || !q) return;
      const mid = { x: (p.x + q.x) / 2, y: (p.y + q.y) / 2 };
      const s = Math.max(0.02, Math.min(400, drag.view.s * (Math.hypot(p.x - q.x, p.y - q.y) / drag.d)));
      view = { s, cx: drag.model.x - (mid.x - w / 2) / s, cy: drag.model.y + (mid.y - h / 2) / s };
      return;
    }
    if (drag?.kind === 'pan') {
      view = { ...view, cx: drag.cx - (at.x - drag.x) / view.s, cy: drag.cy + (at.y - drag.y) / view.s };
      return;
    }
    const m = toModel(at.x, at.y);
    if (tool) { tool.move(m); store.toolRevision++; }
    cursor = m;
    // What a click would take lights up, with a tool for dimensions or
    // constraints and when selecting alike; drawing tools snap instead.
    if (!tool && !drag) hover = hitAt(m);
    if (drag?.kind !== 'press') return;
    const threshold = touch ? 12 : 4;
    if (!drag.moved && Math.hypot(at.x - drag.x, at.y - drag.y) < threshold) return;
    drag.moved = true;
    if (drag.mode === 'tool' || drag.mode === 'action') return;
    if (drag.mode === 'box') { box = { x0: drag.x, y0: drag.y, x1: at.x, y1: at.y }; return; }
    if (drag.mode === 'label') {
      const next = clone(drag.original);
      const target = drag.target;
      const c = next.constraints.find((x) => x.id === target);
      if (c && isDimension(c)) { c.at = [c.at[0] + m.x - drag.start.x, c.at[1] + m.y - drag.start.y]; store.preview(next); }
      return;
    }
    if (drag.mode === 'radius') {
      const target = drag.target!;
      const c = drag.original.entities.find((x) => x.id === target);
      if (c?.kind !== 'circle') return;
      const to = store.gridSnap ? snap(drag.original, m, { reach, grid: minor }) : m;
      const next = dragged(drag.original, [], new Map([[target, Math.max(1e-3, dist(point(drag.original, c.c), to))]]));
      if (next) store.preview(next);
      return;
    }
    let delta = { x: m.x - drag.start.x, y: m.y - drag.start.y };
    if (drag.mode === 'text-scale') {
      // The corner follows the pointer along the diagonal; the opposite corner stays.
      const target = drag.target, from = drag.original.texts?.find((x) => x.id === target);
      if (!from || from.width <= 0 || from.height <= 0) return;
      const k = Math.max(0.05, ((m.x - from.x) * from.width + (m.y - from.y) * from.height) / (from.width ** 2 + from.height ** 2));
      const next = clone(drag.original), at = next.texts!.findIndex((x) => x.id === target);
      next.texts![at] = scaledText(from, k);
      store.preview(next);
      return;
    }
    if (drag.mode === 'text') {
      // A text moves as a whole; the grid catches its corner.
      if (store.gridSnap) delta = { x: Math.round(delta.x / minor) * minor, y: Math.round(delta.y / minor) * minor };
      const next = clone(drag.original), target = drag.target;
      const moving = next.texts?.find((x) => x.id === target), from = drag.original.texts?.find((x) => x.id === target);
      if (moving && from) { moving.x = from.x + delta.x; moving.y = from.y + delta.y; store.preview(next); }
      return;
    }
    if (drag.mode === 'point' && drag.target !== null) {
      const from = drag.origins.get(drag.target)!;
      dragSnap = snap(drag.original, { x: from.x + delta.x, y: from.y + delta.y }, { reach, grid: store.gridSnap ? minor : 0, anchor: from, ignorePoints: new Set([drag.target]) });
      delta = { x: dragSnap.x - from.x, y: dragSnap.y - from.y };
    } else if (store.gridSnap) {
      delta = { x: Math.round(delta.x / minor) * minor, y: Math.round(delta.y / minor) * minor };
    }
    if (!solverReady()) {
      const next = clone(drag.original);
      translate(next, drag.origins, delta);
      repairArcs(next, drag.original);
      store.preview(next);
      return;
    }
    const pulls: Pull[] = [...drag.origins].map(([id, from]) => ({ point: id, x: from.x + delta.x, y: from.y + delta.y }));
    const next = dragged(drag.original, pulls, new Map());
    if (next) store.preview(next);
  }

  function up(e: PointerEvent): void {
    pointers.delete(e.pointerId);
    const d = drag;
    if (d?.kind === 'pinch') { if (pointers.size < 2) drag = null; return; }
    drag = null;
    if (!d || d.kind !== 'press') { dragSnap = null; return; }
    if (d.mode === 'tool') { if (!d.moved || touch) place(); return; }
    if (d.mode === 'action') { if (!d.moved || touch) act(hitAt(d.start), d.shift); return; }
    if (!d.moved) {
      if (d.mode === 'box') { if (!d.shift) store.select([]); return; }
      if ((d.mode === 'label' || d.mode === 'text') && d.target !== null) {
        const now = performance.now();
        if (lastLabel?.id === d.target && now - lastLabel.at < 450) {
          lastLabel = null;
          if (d.mode === 'text') editText(d.target); else editDimension(d.target);
          return;
        }
        lastLabel = { id: d.target, at: now };
      }
      const id = d.mode === 'point' ? hit(store.sketch, d.start, reach) : d.target;
      if (id !== null) store.select([id], d.shift ? 'toggle' : 'replace');
      return;
    }
    if (d.mode === 'box' && box) {
      const a = toModel(box.x0, box.y0), b = toModel(box.x1, box.y1);
      const area = { minX: Math.min(a.x, b.x), maxX: Math.max(a.x, b.x), minY: Math.min(a.y, b.y), maxY: Math.max(a.y, b.y) };
      // Left to right selects what lies inside; right to left whatever the box touches, as in Fusion.
      const window = box.x1 >= box.x0;
      const texts = (store.sketch.texts ?? []).filter((x) => {
        const inside = x.x >= area.minX && x.y >= area.minY && x.x + x.width <= area.maxX && x.y + x.height <= area.maxY;
        const touches = x.x <= area.maxX && x.x + x.width >= area.minX && x.y <= area.maxY && x.y + x.height >= area.minY;
        return window ? inside : touches;
      }).map((x) => x.id);
      const ids = [...store.sketch.entities.filter((x) => (window ? insideBox : touchesBox)(shapeOf(store.sketch, x), area)).map((x) => x.id), ...texts];
      store.select(ids, d.shift ? 'add' : 'replace');
      box = null;
      return;
    }
    if (d.mode === 'text' || d.mode === 'text-scale') { store.commit(clone(store.sketch), d.original); return; }
    const finished = clone(store.sketch);
    if (d.mode === 'point' && d.target !== null && dragSnap?.point !== undefined) merge(finished, d.target, dragSnap.point);
    dragSnap = null;
    const problem = store.settle(finished, d.original);
    if (problem) { store.preview(d.original); ui.say(problem, true); }
  }

  function cancelPointer(e: PointerEvent): void {
    pointers.delete(e.pointerId);
    if (drag?.kind === 'press' && drag.moved && !['tool', 'action', 'box'].includes(drag.mode)) store.preview(drag.original);
    drag = null;
    box = null;
    dragSnap = null;
  }

  function wheel(e: WheelEvent): void {
    e.preventDefault();
    const at = local(e);
    const m = toModel(at.x, at.y);
    const s = Math.max(0.02, Math.min(400, view.s * Math.exp(-e.deltaY * 0.0015)));
    view = { s, cx: m.x - (at.x - w / 2) / s, cy: m.y + (at.y - h / 2) / s };
  }

  function doubleClick(e: MouseEvent): void {
    const note = annotationAt(e);
    if (note !== null) { editDimension(note); return; }
    if (tool || action) return;
    const at = local(e);
    const text = textAt(store.sketch, toModel(at.x, at.y));
    if (text !== null && hit(store.sketch, toModel(at.x, at.y), reach) === null) { editText(text); return; }
    const id = hit(store.sketch, toModel(at.x, at.y), reach);
    if (id !== null) store.select(chain(store.sketch, id), e.shiftKey ? 'add' : 'replace');
  }

  /** A dimension's new value, typed on the keypad, solved before it is kept. */
  function editDimension(id: Id): void {
    const c = store.sketch.constraints.find((x) => x.id === id);
    if (!c || !isDimension(c)) return;
    if (c.driven) { ui.say(t("A driven dimension follows the geometry; delete another dimension to drive this one.")); return; }
    const angle = c.kind === 'angle';
    osk.number(angle ? t("Angle") : c.kind === 'diameter' ? t("Diameter") : c.kind === 'radius' ? t("Radius") : t("Distance"), c.value, angle ? '°' : 'mm', (v) => {
      if (!(v > 0)) { ui.say(t("A dimension must be above zero."), true); return; }
      const problem = store.change((s) => { const x = s.constraints.find((k) => k.id === id); if (x && isDimension(x)) x.value = v; });
      if (problem) ui.say(problem, true);
    });
  }

  // ---- Placing points, acting, and the heads-up fields --------------------------------------------
  function resetEntry(): void { locked = {}; editing = null; active = 0; store.toolRevision++; }

  function place(): void {
    if (!tool) return;
    if (!commitEditing()) return;
    const at = cursor ? tool.resolve(snap(store.sketch, cursor, { reach, grid: store.gridSnap ? minor : 0, anchor: tool.anchor(), tangent: tool.id === 'line' }), locked) : null;
    if (!at) return;
    const draft = clone(store.sketch);
    const outcome = tool.click(draft, at, locked);
    if (outcome === 'ignored') return;
    if (draft.next !== store.sketch.next) {
      const problem = store.settle(draft);
      if (problem) ui.say(problem, true);
    }
    resetEntry();
  }

  function apply(step: Step): void {
    if (step.kind === 'select') { store.select(step.ids); resetEntry(); return; }
    if (step.kind === 'toggle') { store.select([step.id], 'toggle'); store.toolRevision++; return; }
    if (step.kind === 'error') { ui.say(step.message, true); resetEntry(); return; }
    if (step.kind === 'apply') {
      // A constraint or dimension that only repeats what holds already is refused, as Fusion does.
      const strict = !!action && !MODIFY.some((m) => m.id === action.id);
      const problem = store.settle(step.sketch, store.sketch, strict);
      if (problem?.includes('driven dimension')) ui.say(problem);
      else if (problem) ui.say(problem, true);
      else if (step.edit !== undefined) editDimension(step.edit);
      else if (action && !MODIFY.some((m) => m.id === action.id)) ui.say(action.id === 'dimension' ? t("Dimension added") : t("{0} added", RELATE.find((r) => r.id === action.id)?.label ?? 'Constraint'));
      else {
        if (step.select) store.select(step.select);
        if (step.done) store.act(null);
      }
    }
    resetEntry();
  }

  /** A click in a modify, constraint or dimension tool. Shift-click picks
   *  what the selection tools work on. */
  function act(at: Hit, shift: boolean): void {
    if (!action) return;
    if (!commitEditing()) return;
    if (shift && at.entity !== null && ['offset', 'mirror', 'rpattern', 'cpattern'].includes(action.id)) {
      store.select([at.entity], 'toggle');
      resetEntry();
      return;
    }
    apply(action.click(ctx, at, locked));
  }

  function enter(): void {
    if (!commitEditing()) return;
    if (tool) place();
    else if (action && cursor) apply(action.enter(ctx, cursor, locked));
  }

  function parsed(field: Field, text: string): number | null {
    const v = evaluate(text);
    if (v === null) return null;
    if (field.kind === 'length') return v > 0 ? toSource(v, 'mm') : null;
    if (field.kind === 'count') return v >= 1 ? Math.round(v) : null;
    return v;
  }

  function commitEditing(): boolean {
    if (!editing) return true;
    const field = fields.find((f) => f.key === editing!.key);
    const v = field ? parsed(field, editing.text) : null;
    if (!field || v === null || !Number.isFinite(v)) {
      ui.say(t("Enter a valid value before applying the tool."), true);
      return false;
    }
    locked = { ...locked, [field.key]: v };
    editing = null;
    return true;
  }

  function shown(field: Field): string {
    if (editing?.key === field.key) return editing.text;
    const v = locked[field.key] ?? field.value;
    if (field.kind === 'length') return displayNumber(v, 'mm');
    if (field.kind === 'angle') return `${v.toFixed(1)}`;
    return String(Math.round(v));
  }
  const suffix = (field: Field): string => (field.kind === 'length' ? unitLabel('mm') : field.kind === 'angle' ? '°' : '');

  function tapField(field: Field, i: number): void {
    active = i;
    const value = locked[field.key] ?? field.value;
    const unit = field.kind === 'length' ? 'mm' : field.kind === 'angle' ? '°' : '';
    osk.number(field.label, field.kind === 'count' ? Math.round(value) : value, unit, (v) => {
      const ok = field.kind === 'length' ? v > 0 : field.kind === 'count' ? v >= 1 : true;
      if (ok) locked = { ...locked, [field.key]: field.kind === 'count' ? Math.round(v) : v };
    });
  }
  const hudAt = $derived(resolved ?? cursor);

  // ---- Keyboard ---------------------------------------------------------------------------------
  let spaceHeld = false;

  function escape(): void {
    if (editing) { editing = null; return; }
    if (Object.keys(locked).length) { locked = {}; return; }
    if (tool?.busy) { tool.reset(); store.toolRevision++; return; }
    if (action?.busy) { action.reset(); store.toolRevision++; return; }
    if (tool || action) { choose(null); return; }
    store.select([]);
  }

  function key(e: KeyboardEvent): void {
    if (osk.open || ui.modal || e.isComposing) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest('input, textarea, select, [contenteditable]')) return;
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === ' ') { spaceHeld = true; return; }
    if (mod && e.key.toLowerCase() === 'z') { e.preventDefault(); if (e.shiftKey) store.redo(); else store.undo(); return; }
    if (mod && e.key.toLowerCase() === 'y') { e.preventDefault(); store.redo(); return; }
    if (mod && e.key.toLowerCase() === 'a') { e.preventDefault(); store.select(store.sketch.entities.map(e => e.id)); return; }
    if (mod || e.altKey) return;
    if (e.key === 'Escape') { e.preventDefault(); escape(); return; }
    if (fields.length && (/^[0-9.,+\-*/()]$/.test(e.key) || (editing && e.key === 'Backspace'))) {
      e.preventDefault();
      const field = fields[Math.min(active, fields.length - 1)]!;
      const text = editing?.key === field.key ? editing.text : '';
      editing = { key: field.key, text: e.key === 'Backspace' ? text.slice(0, -1) : text + e.key };
      return;
    }
    if (e.key === 'Tab' && (tool || action)) {
      e.preventDefault();
      if (!fields.length) return;
      if (!commitEditing()) return;
      active = (Math.min(active, fields.length - 1) + (e.shiftKey ? fields.length - 1 : 1)) % fields.length;
      return;
    }
    if (e.key === 'Enter' && (tool || action)) { e.preventDefault(); enter(); return; }
    if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); store.deleteSelection(); return; }
    const k = e.key.toLowerCase();
    if (TOOL_KEYS[k]) { choose(TOOL_KEYS[k]!); return; }
    if (ACTION_KEYS[k]) { chooseAction(ACTION_KEYS[k]!); return; }
    if (k === 'x') { const problem = store.toggleConstruction(); if (problem) ui.say(problem, true); return; }
    if (k === 'f') { fit(); return; }
    if (k === 'g') { store.gridSnap = !store.gridSnap; }
  }
  function keyUp(e: KeyboardEvent): void { if (e.key === ' ') spaceHeld = false; }

  function choose(id: ToolId | null): void {
    store.use(id);
    hover = null;
    resetEntry();
  }
  function chooseAction(id: ActionId): void {
    if (action?.id === id) { choose(null); return; }
    store.act(id);
    hover = null;
    resetEntry();
  }

  // ---- Leaving the sketch ------------------------------------------------------------------------
  let sending = $state(false);
  const fileName = $derived(`${store.name.replace(/[\\/:*?"<>|]/g, ' ').trim() || 'Sketch'}.dxf`);

  async function send(): Promise<void> {
    if (sending || !hasCut(store.sketch)) return;
    sending = true;
    try {
      if(store.target==='replace' && (!store.sourcePart || store.jobKey!==server.doc?.draft?.key))throw new Error(t('The edited job changed. Open the part again before replacing it.'));
      const bytes = new TextEncoder().encode(toDxf(store.sketch));
      const { id, warnings } = await api.importPart(fileName, bytes.buffer as ArrayBuffer);
      await api.savePartSketch(id,$state.snapshot(store.sketch));
      ui.selected = id;
      if(store.target==='replace' && store.sourcePart && server.doc?.draft) await api.replaceDraftPart(store.sourcePart,id,server.doc.draft.revision);
      else if (store.target === 'add' && server.doc?.draft) await api.addParts([id]);
      else await api.openPart(id);
      store.bindPart(id);
      ui.tab = 'setup';
      ui.say(store.target === 'replace' ? t('Part saved in the job. Check the settings of changed contours.') : warnings?.length ? t("Sketch added as a part · {0}", warnings.join(' · ')) : t("Sketch added as a part. Choose a material to prepare it for cutting."));
    } catch (reason) {
      ui.say(explain(reason), true);
    } finally {
      sending = false;
    }
  }

  function download(): void {
    const url = URL.createObjectURL(new Blob([toDxf(store.sketch)], { type: 'application/dxf' }));
    const a = document.createElement('a');
    a.href = url;
    a.download = fileName;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  // ---- What the side panel reports ---------------------------------------------------------------
  const selected = $derived(store.sketch.entities.filter((e) => store.selection.has(e.id)));
  const selectedNotes = $derived(store.sketch.constraints.filter((c) => store.selection.has(c.id)));
  /** The one selected text's scale handle: its upper right corner, in millimetres. */
  function scaleHandle(): { id: Id; x: number; y: number } | null {
    const only = store.selection.size === 1 ? store.sketch.texts?.find((x) => store.selection.has(x.id)) : undefined;
    return only ? { id: only.id, x: only.x + only.width, y: only.y + only.height } : null;
  }
  const selectedTexts = $derived((store.sketch.texts ?? []).filter((x) => store.selection.has(x.id)));
  const NOTE_NAME: Record<Constraint['kind'], string> = {
    horizontal: 'Horizontal', vertical: 'Vertical', parallel: 'Parallel', perpendicular: 'Perpendicular', equal: 'Equal', collinear: 'Collinear',
    tangent: 'Tangent', concentric: 'Concentric', midpoint: 'Midpoint', on: 'Coincident', symmetric: 'Symmetric', fix: 'Fixed',
    distance: 'Distance', offset: 'Distance to line', diameter: 'Diameter', radius: 'Radius', angle: 'Angle',
  };
  const facts = $derived.by(() => {
    if (selected.length !== 1) return [] as Array<[string, string]>;
    const e = selected[0]!;
    const s = store.sketch;
    if (e.kind === 'line') {
      const a = point(s, e.a), b = point(s, e.b);
      const angle = angleOf(a, b) * 180 / Math.PI;
      return [['Line', ''], ['Length', `${displayNumber(dist(a, b), 'mm')} ${unitLabel('mm')}`], ['Angle', `${(angle < 0 ? angle + 360 : angle).toFixed(2)}°`]] as Array<[string, string]>;
    }
    if (e.kind === 'circle') return [['Circle', ''], ['Diameter', `${displayNumber(2 * e.r, 'mm')} ${unitLabel('mm')}`]] as Array<[string, string]>;
    const c = point(s, e.c), a = point(s, e.a), b = point(s, e.b);
    const r = dist(c, a);
    const sw = sweep(angleOf(c, a), angleOf(c, b));
    return [['Arc', ''], ['Radius', `${displayNumber(r, 'mm')} ${unitLabel('mm')}`], ['Sweep', `${(sw * 180 / Math.PI).toFixed(2)}°`], ['Length', `${displayNumber(r * sw, 'mm')} ${unitLabel('mm')}`]] as Array<[string, string]>;
  });
  const status = $derived.by(() => {
    void store.solverRevision;
    if (!solverReady()) return 'Loading the constraint solver…';
    if (!store.sketch.entities.length) return store.sketch.texts?.length ? 'Text only' : 'Empty sketch';
    if (!free) return 'Checking…';
    const loose = store.sketch.entities.filter((e) => !defined(e, free)).length;
    return loose ? t("{0} of {1} curves can still move (blue)", loose, store.sketch.entities.length) : t("Fully defined");
  });

  const snapLabel: Record<string, string> = { end: 'Endpoint', center: 'Center', mid: 'Midpoint', intersection: 'Intersection', on: 'On curve', tangent: 'Tangential', horizontal: 'Horizontal', vertical: 'Vertical', grid: 'Grid', free: '' };
  const marker = $derived(dragSnap ?? resolved);
  const coords = $derived(resolved ?? cursor);
  const fixedGrid = $derived(store.gridStep !== null);
  function setGrid(step: number | null): void { store.gridStep = step && step > 0 ? step : null; }
</script>

{#if textOpen}<CreateText onclose={()=>{textOpen=false;editingText=null;}} editing={editingText} onplace={placeText} />{/if}
<svelte:window onkeydown={key} onkeyup={keyUp} />

<section class="panel main sketch-main">
  <div class="sketch-bar" role="toolbar" aria-label={t("Sketch tools")}>
    <div class="sk-group">
      <span class="sk-title">{t("Create")}</span>
      <div class="sk-tools">
        <button class="sk-tool" title={t("Text with fonts")} aria-label={t("Text with fonts")} onclick={()=>textOpen=true}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5h16M12 5v15M8 20h8M4 5v3M20 5v3" /></svg></button>
        <button class="sk-tool" title={t("Import DXF / SVG")} aria-label={t("Import DXF / SVG")} disabled={importing} onclick={()=>importInput.click()}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14 3H5v18h14V8l-5-5v5h5M8 13h8M13 10l3 3-3 3" /></svg></button>
        <input bind:this={importInput} type="file" accept=".dxf,.svg" hidden onchange={importDrawing} />
        {#each CREATE as t (t.label)}
          <button class="sk-tool" class:on={!action && (tool?.id ?? null) === t.id} title={t.key ? `${t.label} (${t.key})` : t.label} aria-label={t.label} onclick={() => choose(t.id)}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d={t.icon} /></svg>
          </button>
        {/each}
      </div>
    </div>
    <div class="sk-group">
      <span class="sk-title">{t("Modify")}</span>
      <div class="sk-tools">
        {#each MODIFY as t (t.id)}
          <button class="sk-tool" class:on={action?.id === t.id} title={t.key ? `${t.label} (${t.key})` : t.label} aria-label={t.label} onclick={() => chooseAction(t.id)}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d={t.icon} /></svg>
          </button>
        {/each}
        <button class="sk-tool" title={t("Construction (X)")} aria-label={t("Toggle construction")} disabled={!selected.length} onclick={() => { const p = store.toggleConstruction(); if (p) ui.say(p, true); }}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20L20 4" stroke-dasharray="3 3" /></svg>
        </button>

        <button class="sk-tool" title={t("Delete (Del)")} aria-label={t("Delete")} disabled={!store.selection.size} onclick={() => store.deleteSelection()}><i class="ic ic-trash"></i></button>
        <button class="sk-tool" title={t("Undo (Ctrl+Z)")} aria-label={t("Undo")} disabled={!store.canUndo} onclick={() => store.undo()}><i class="ic ic-undo"></i></button>
        <button class="sk-tool" title={t("Redo (Ctrl+Y)")} aria-label={t("Redo")} disabled={!store.canRedo} onclick={() => store.redo()}><i class="ic ic-redo"></i></button>
        <button class="sk-tool" title={t("Fit (F)")} aria-label={t("Fit")} onclick={fit}><i class="ic ic-fit"></i></button>
      </div>
    </div>
    <div class="sk-group">
      <span class="sk-title">{t("Constraints")}</span>
      <div class="sk-tools">
        {#each RELATE as t (t.id)}
          <button class="sk-tool glyph" class:on={action?.id === t.id} title={t.label} aria-label={t.label} onclick={() => chooseAction(t.id)}>{t.glyph}</button>
        {/each}

        <button class="sk-tool" class:on={action?.id === 'dimension'} title={t("Sketch dimension (D)")} aria-label={t("Sketch dimension")} onclick={() => chooseAction('dimension')}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 8v10M20 8v10M4 13h16M4 13l3-2M4 13l3 2M20 13l-3-2M20 13l-3 2" /></svg>
        </button>
      </div>
    </div>


  </div>

  <div class="sketch-canvas" bind:clientWidth={w} bind:clientHeight={h}>
    <svg bind:this={svg} width={w} height={h} class:drawing={!!tool || !!action} role="application" aria-label={t("Sketch canvas")}
      onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={cancelPointer}
      onpointerleave={() => { if (!drag) { cursor = null; hover = null; } }} onwheel={wheel} ondblclick={doubleClick}
      oncontextmenu={(e) => { e.preventDefault(); escape(); }}>
      <g class="grid">{#each gridLines as g, i (i)}<path d={g.d} class:major={g.major} />{/each}</g>
      <path class="axis-x" d={`M0 ${origin.y}H${w}`} />
      <path class="axis-y" d={`M${origin.x} 0V${h}`} />
      {#each drawn as e (e.id)}
        <path d={e.d} class="curve" class:defined={e.defined} class:construction={e.construction} class:selected={store.selection.has(e.id)}
          class:hovered={!tool && hover?.entity === e.id && hover?.point === null} class:picked={picked.curves.includes(e.id)} />
      {/each}
      {#each store.sketch.texts ?? [] as tx (tx.id)}
        <path d={textPath(tx)} class="text-shape" class:selected={store.selection.has(tx.id)} fill-rule="evenodd" />
      {/each}
      {#if selectedTexts.length === 1 && store.selection.size === 1}
        {@const tx = selectedTexts[0]!}
        {@const lo = toScreen({ x: tx.x, y: tx.y })}
        {@const hi = toScreen({ x: tx.x + tx.width, y: tx.y + tx.height })}
        <rect class="text-box" x={Math.min(lo.x, hi.x)} y={Math.min(lo.y, hi.y)} width={Math.abs(hi.x - lo.x)} height={Math.abs(hi.y - lo.y)} />
        <rect class="text-scale" x={hi.x - 5} y={hi.y - 5} width="10" height="10"><title>{t("Drag to scale the text")}</title></rect>
      {/if}
      {#each handles as p (p.id)}
        {#if p.center}
          <!-- A centre answers the cursor like an end point, so it can be dimensioned. -->
          {#if !tool && (hover?.point === p.id || picked.points.includes(p.id) || store.selection.has(p.id))}<circle class="center-ring" class:picked={picked.points.includes(p.id)} cx={p.x} cy={p.y} r="7" />{/if}
          <path class="handle center" class:defined={p.defined} class:hovered={!tool && hover?.point === p.id} class:picked={picked.points.includes(p.id)} d={`M${p.x - 4} ${p.y}h8M${p.x} ${p.y - 4}v8`} />
        {:else}<rect class="handle" class:defined={p.defined} class:fixed={p.fixed} class:hovered={!tool && hover?.point === p.id} class:picked={picked.points.includes(p.id)} x={p.x - 2.5} y={p.y - 2.5} width="5" height="5" />{/if}
      {/each}

      {#each dims as d (d.id)}
        <g class="dim" class:selected={store.selection.has(d.id)} class:conflict={store.conflicts.has(d.id)}>
          <path class="dim-line" d={d.d} />
          {#each d.arrows as a, i (i)}<path class="dim-arrow" d={a} />{/each}
          <g data-constraint={d.id} class="dim-label">
            <rect x={d.text.x - 4 * d.label.length - 6} y={d.text.y - 11} width={8 * d.label.length + 12} height="22" rx="5" />
            <text x={d.text.x} y={d.text.y + 4} text-anchor="middle">{d.label}</text>
          </g>
        </g>
      {/each}
      {#each marks as m (`${m.id}-${m.x}-${m.y}`)}
        <g class="glyph" class:conflict={store.conflicts.has(m.id)} class:selected={store.selection.has(m.id)} data-constraint={m.id}>
          <rect x={m.x - 8} y={m.y - 8} width="16" height="16" rx="4" />
          <text x={m.x} y={m.y + 4} text-anchor="middle">{m.text}</text>
        </g>
      {/each}

      {#if ghost}
        {#each ghost.construction as s, i (i)}<path d={pathOf(s)} class="ghost construction" />{/each}
        {#each ghost.shapes as s, i (i)}<path d={pathOf(s)} class="ghost" />{/each}
        {#each ghost.points as p, i (i)}{@const q = toScreen(p)}<circle cx={q.x} cy={q.y} r="2.5" class="ghost-point" />{/each}
      {/if}
      {#if preview}
        {#each preview.shapes as s, i (i)}<path d={pathOf(s)} class="ghost" />{/each}
        {#each preview.removed as s, i (i)}<path d={pathOf(s)} class="removed" />{/each}
      {/if}
      {#if pendingDim}
        <g class="dim pending">
          <path class="dim-line" d={pendingDim.d} />
          {#each pendingDim.arrows as a, i (i)}<path class="dim-arrow" d={a} />{/each}
          <text x={pendingDim.text.x} y={pendingDim.text.y + 4} text-anchor="middle">{pendingDim.label}</text>
        </g>
      {/if}
      {#if anchor && resolved && (resolved.kind === 'horizontal' || resolved.kind === 'vertical' || resolved.kind === 'tangent')}
        {@const a = toScreen(anchor)}{@const b = toScreen(resolved)}<path class="inference" d={`M${a.x} ${a.y}L${b.x} ${b.y}`} />
      {/if}
      {#if marker && marker.kind !== 'free'}
        {@const q = toScreen(marker)}
        {#if marker.kind === 'end'}<rect class="snap" x={q.x - 6} y={q.y - 6} width="12" height="12" />
        {:else if marker.kind === 'center'}<circle class="snap" cx={q.x} cy={q.y} r="7" />
        {:else if marker.kind === 'tangent'}<circle class="snap" cx={q.x} cy={q.y} r="6" /><path class="snap" d={`M${q.x - 8} ${q.y + 6}H${q.x + 8}`} />
          {#if marker.tangent}{@const contact = toScreen(marker.tangent.contact)}<circle class="snap" cx={contact.x} cy={contact.y} r="4" />{/if}
        {:else if marker.kind === 'mid'}<path class="snap" d={`M${q.x} ${q.y - 7}L${q.x + 7} ${q.y + 5}H${q.x - 7}Z`} />
        {:else if marker.kind === 'intersection'}<path class="snap" d={`M${q.x - 6} ${q.y - 6}L${q.x + 6} ${q.y + 6}M${q.x + 6} ${q.y - 6}L${q.x - 6} ${q.y + 6}`} />
        {:else if marker.kind === 'on'}<path class="snap" d={`M${q.x} ${q.y - 7}L${q.x + 7} ${q.y}L${q.x} ${q.y + 7}L${q.x - 7} ${q.y}Z`} />
        {:else if marker.kind === 'grid' || marker.kind === 'horizontal' || marker.kind === 'vertical'}<path class="snap grid-snap" d={`M${q.x - 5} ${q.y}H${q.x + 5}M${q.x} ${q.y - 5}V${q.y + 5}`} /><rect class="snap grid-snap" x={q.x - 3} y={q.y - 3} width="6" height="6" />{/if}
      {/if}
      {#if box}
        <rect class="box" class:crossing={box.x1 < box.x0} x={Math.min(box.x0, box.x1)} y={Math.min(box.y0, box.y1)} width={Math.abs(box.x1 - box.x0)} height={Math.abs(box.y1 - box.y0)} />
      {/if}
    </svg>

    {#if fields.length && hudAt}
      {@const q = toScreen(hudAt)}
      <div class="hud" style:left={`${Math.min(q.x + 18, w - 170)}px`} style:top={`${Math.max(8, Math.min(q.y + 18, h - 44 * fields.length - 40))}px`}>
        {#each fields as f, i (f.key)}
          <button class="hud-field" class:active={i === Math.min(active, fields.length - 1)} class:locked={f.key in locked || editing?.key === f.key} data-numpad onclick={() => tapField(f, i)}>
            <span class="hud-label">{t(f.label)}</span><span class="hud-value">{shown(f)}{#if editing?.key === f.key}<span class="caret"></span>{/if}</span><span class="hud-unit">{suffix(f)}</span>
          </button>
        {/each}
      </div>
    {/if}

    <div class="sketch-status">
      <span>{t(hint)}</span>
      <span class="sketch-coords">{#if marker && snapLabel[marker.kind]}<b>{t(snapLabel[marker.kind])}</b>{/if}{#if coords}X {displayNumber(coords.x, 'mm')} · Y {displayNumber(coords.y, 'mm')} {unitLabel('mm')}{/if}</span>
    </div>
  </div>
</section>

<aside class="panel side sketch-side">
  <label for="sketch-archive">{t("Saved sketches")}</label>
  <div class="sketch-archive-row">
    <select id="sketch-archive" value={store.activeDocument} onchange={async e=>{if (await store.openDocument(e.currentTarget.value)) fit(); else ui.say(t('Could not load sketches.'), true);}}>{#each store.documents as document}<option value={document.id}>{document.name}</option>{/each}</select>
    <button class="sk-tool" title={t('Remove chosen sketch: {0}', store.name)} aria-label={t('Remove chosen sketch: {0}', store.name)} onclick={()=>{store.removeDocument();fit();}}><i class="ic ic-trash" aria-hidden="true"></i></button>
  </div>
  {#if store.removedDocuments.length}<button class="btn btn-soft" onclick={()=>{store.restoreDocument();fit();}}>{t('Undo remove: {0}', store.removedDocuments.at(-1)?.name)}</button>{/if}
  <button class="btn btn-ghost" onclick={()=>beginProject('new')}>{t("New sketch")}</button>
  <div class="sk-name"><span>{t("Name")}</span><button class="sk-name-field" onclick={() => osk.text(t("Sketch name"), store.name, (v) => store.rename(v))}>{store.name}</button></div>

  <div class="sk-facts">
    <h3>{t("Sketch")}</h3>
    <p class="sk-status" class:done={status === t("Fully defined")}>{t(status)}</p>
  </div>

  <div class="sk-facts">
    <h3>{t("Selection")}</h3>
    {#if selectedTexts.length === 1 && !selected.length && !selectedNotes.length}
      {@const tx = selectedTexts[0]!}
      <dl><dt class="kind">{t("Text")}</dt><dd>{tx.value.split('\n')[0]}</dd>
        <dt>{t("Font size")}</dt><dd>{displayNumber(tx.size, 'mm')} {unitLabel('mm')}</dd>
        <dt>{t("Size")}</dt><dd>{displayNumber(tx.width, 'mm')} × {displayNumber(tx.height, 'mm')} {unitLabel('mm')}</dd></dl>
      <button class="btn btn-ghost block sk-small" onclick={() => editText(tx.id)}>{t("Edit text")}</button>
      <p class="muted">{t("Drag to move · corner handle scales · double click to edit · Del removes it.")}</p>
    {:else if selectedNotes.length === 1 && !selected.length}
      {@const n = selectedNotes[0]!}
      <dl><dt class="kind">{t(NOTE_NAME[n.kind])}</dt><dd>{#if isDimension(n)}{dimensionLabel(n.driven ? { ...n, value: measure(store.sketch, n) } : n, mm)}{/if}</dd></dl>
      {#if isDimension(n)}<button class="btn btn-ghost block sk-small" onclick={() => editDimension(n.id)}>{t("Change value")}</button>{/if}
      <p class="muted">{t("Del removes it.")}</p>
    {:else if !selected.length && !selectedNotes.length}<p class="muted">{store.sketch.entities.length} {t("curves. Nothing selected.")}</p>
    {:else if facts.length && !selectedNotes.length}
      <dl>{#each facts as [k, v] (k)}{#if v}<dt>{t(k)}</dt><dd>{v}</dd>{:else}<dt class="kind">{t(k)}</dt><dd></dd>{/if}{/each}</dl>
    {:else}<p>{selected.length} {t("curves")}{selectedNotes.length ? t(", {0} constraints", selectedNotes.length) : ''} {t("selected")}</p>{/if}
  </div>

  <div class="sk-facts">
    <h3>{t("Grid and display")}</h3>
    <div class="sk-row">
      <span>{t("Grid")}</span>
      <div class="seg" role="group" aria-label={t("Grid")}>
        <button class:on={!fixedGrid} onclick={() => setGrid(null)}>{t("Adaptive")}</button>
        <button class:on={fixedGrid} onclick={() => setGrid(store.gridStep ?? minor)}>{t("Fixed")}</button>
      </div>
    </div>
    {#if fixedGrid}
      <div class="sk-row">
        <span>{t("Spacing")}</span>
        <button class="sk-value" data-numpad onclick={() => osk.number('Grid spacing', store.gridStep ?? minor, 'mm', (v) => setGrid(v))}>{displayNumber(store.gridStep ?? minor, 'mm')} {unitLabel('mm')}</button>
      </div>
    {:else}<p class="muted">{t("Now")} {displayNumber(minor, 'mm')} {unitLabel('mm')}{t(", following the zoom.")}</p>{/if}
    <label class="sk-check"><input type="checkbox" bind:checked={store.gridSnap} /> {t("Snap to grid (G)")}</label>
    <label class="sk-check"><input type="checkbox" bind:checked={store.automaticConstraints} onchange={()=>store.use(store.tool?.id ?? null)} /> {t("Add further relations automatically")}</label>
    <p class="muted">{t("By default only joined end points and the basic shape. Set tangents and other relations yourself when needed.")}</p>
    <label class="sk-check"><input type="checkbox" bind:checked={store.showConstraints} /> {t("Show constraints")}</label>
    <label class="sk-check"><input type="checkbox" bind:checked={store.showDimensions} /> {t("Show dimensions")}</label>
  </div>

  <details class="sk-keys">
    <summary>{t("Keyboard")}</summary>
    <dl>
      <dt>L R C A P</dt><dd>{t("Line, rectangle, circle, arc, polygon")}</dd>
      <dt>D T O</dt><dd>{t("Dimension, trim, offset")}</dd>
      <dt>{t("Type a number")}</dt><dd>{t("Locks the highlighted value; 50/2 works")}</dd>
      <dt>Tab · Enter</dt><dd>{t("Next value · place or apply")}</dd>
      <dt>{t("Esc · right click")}</dt><dd>{t("Clear values, end the tool, then Select")}</dd>
      <dt>{t("Double click")}</dt><dd>{t("A dimension: change it. A curve: its profile")}</dd>
      <dt>Shift+click</dt><dd>{t("In offset, mirror and patterns: add to the selection")}</dd>
      <dt>X · Del</dt><dd>{t("Construction · delete curves, constraints, dimensions")}</dd>
      <dt>F · G</dt><dd>{t("Fit · grid snap")}</dd>
      <dt>{t("Wheel · middle drag")}</dt><dd>{t("Zoom · pan (also Space+drag, two fingers)")}</dd>
    </dl>
  </details>

  <div class="sk-actions">
    <p class="muted">{t(store.target === 'replace' ? "Replaces this part in the open job. Original stays available; Undo restores it. Bridges are cleared and changed contour settings must be reviewed." : store.target === 'new' ? "Creates a new project from this sketch. Existing jobs remain available." : "Adds this sketch to the open project.")}</p>
    <button class="btn btn-primary lg block" disabled={sending || !hasCut(store.sketch)} onclick={send}>{sending ? t("Adding…") : t(store.target === 'replace' ? "Replace part in job" : store.target === 'new' ? "Create project from sketch" : "Add to parts")}</button>
    <button class="btn btn-ghost block" disabled={!hasCut(store.sketch)} onclick={download}>{t("Download DXF")}</button>
    <button class="btn btn-ghost block" disabled={!store.sketch.entities.length} onclick={() => store.clear()}>{t("Clear this sketch")}</button>
  </div>
</aside>

<style>
  .sketch-archive-row { display:grid; grid-template-columns:minmax(0,1fr) 44px; gap:6px; align-items:center; }
  .sketch-archive-row .sk-tool { border-color:var(--line); background:var(--panel-2); }
  .sketch-side select{width:100%;min-height:44px;color:var(--ink);background:var(--panel-2);border:1px solid var(--line);padding:8px;margin:6px 0}
  .sketch-main { position: relative; }
  .sketch-bar { display: flex; flex-wrap: wrap; gap: 14px; padding: 10px; border-bottom: 1px solid var(--line); background: var(--panel-2); }
  .sk-group { display: flex; flex-direction: column; gap: 6px; padding: 8px; border: 1px solid var(--line); border-radius: 10px; background: var(--panel); }
  .sk-title { font-size: var(--t-sm); font-weight: 700; letter-spacing: .06em; text-transform: uppercase; color: var(--ink-3); padding-left: 4px; }
  .sk-tools { display: grid; grid-template-rows: repeat(2, 44px); grid-auto-flow: column; grid-auto-columns: 44px; gap: 3px; }
  .sk-tool { min-width: 44px; height: 44px; border-radius: 9px; border: 1px solid transparent; background: transparent; display: inline-flex; align-items: center; justify-content: center; cursor: pointer; color: var(--ink-2); }
  .sk-tool.glyph { font-weight: 700; font-size: 15px; padding: 0 6px; }
  .sk-tool:hover:not(:disabled) { background: var(--panel); border-color: var(--line); }
  .sk-tool.on { background: var(--accent-soft); color: var(--accent-2); border-color: transparent; }
  .sk-tool:disabled { opacity: .35; cursor: not-allowed; }
  .sk-tool svg { width: 22px; height: 22px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  .sketch-canvas { position: relative; flex: 1; min-height: 0; overflow: hidden; background: var(--panel); }
  .sketch-canvas svg { display: block; touch-action: none; cursor: default; }
  .sketch-canvas svg.drawing { cursor: crosshair; }
  .grid path { stroke: var(--grid); stroke-width: 1; }
  .grid path.major { stroke: var(--grid-strong); }
  .axis-x { stroke: #d9534f; stroke-width: 1; opacity: .7; }
  .axis-y { stroke: #3f9a5a; stroke-width: 1; opacity: .7; }
  .curve { fill: none; stroke: var(--sketch); stroke-width: 1.8; stroke-linecap: round; }
  .curve.defined { stroke: var(--ink); }
  .curve.construction { stroke: var(--sketch-construction); stroke-dasharray: 6 4; stroke-width: 1.3; }
  .curve.hovered { stroke: var(--accent); stroke-width: 3.2; opacity: .85; }
  .curve.selected { stroke: var(--accent); stroke-width: 3; }
  .text-shape { fill: color-mix(in srgb, var(--sketch) 12%, transparent); stroke: var(--sketch); stroke-width: 1.4; stroke-linejoin: round; }
  .text-shape.selected { stroke: var(--accent); stroke-width: 2.4; fill: color-mix(in srgb, var(--accent) 16%, transparent); }
  .text-box { fill: none; stroke: var(--accent); stroke-width: 1; stroke-dasharray: 4 3; pointer-events: none; }
  .text-scale { fill: var(--panel); stroke: var(--accent); stroke-width: 2; cursor: nesw-resize; }
  .curve.picked { stroke: var(--ok); stroke-width: 3.5; }
  .handle.picked { stroke: var(--ok); fill: var(--ok); }
  .handle { fill: var(--panel); stroke: var(--sketch); stroke-width: 1.2; }
  .handle.defined { stroke: var(--ink); }
  .handle.fixed { fill: var(--ink); }
  .handle.hovered { stroke: var(--accent); stroke-width: 2.4; }
  .handle.center { fill: none; stroke-width: 1.6; }
  .handle.center.hovered { stroke: var(--accent); stroke-width: 2.6; }
  .handle.center.picked { stroke: var(--ok); }
  .center-ring { fill: color-mix(in srgb, var(--accent) 14%, transparent); stroke: var(--accent); stroke-width: 1.4; pointer-events: none; }
  .center-ring.picked { stroke: var(--ok); fill: color-mix(in srgb, var(--ok) 16%, transparent); }
  .dim-line { fill: none; stroke: var(--ink-2); stroke-width: 1; }
  .dim-arrow { fill: var(--ink-2); }
  .dim-label { cursor: pointer; }
  .dim-label rect { fill: var(--panel); stroke: var(--line-2); }
  .dim text { font-size: 13px; font-weight: 600; fill: var(--ink); font-variant-numeric: tabular-nums; }
  .dim.selected .dim-line { stroke: var(--accent); }
  .dim.selected .dim-arrow { fill: var(--accent); }
  .dim.selected .dim-label rect { stroke: var(--accent); }
  .dim.pending .dim-line { stroke: var(--accent); stroke-dasharray: 4 3; }
  .dim.pending .dim-arrow, .dim.pending text { fill: var(--accent-2); }
  .dim.conflict .dim-label rect, .glyph.conflict rect { stroke:var(--danger); stroke-width:2; }
  .dim.conflict .dim-label text, .glyph.conflict text { fill:var(--danger); }
  .glyph { cursor: pointer; }
  .glyph rect { fill: color-mix(in srgb, var(--panel) 90%, transparent); stroke: var(--line-2); }
  .glyph text { font-size: 11px; font-weight: 700; fill: var(--ink-2); }
  .glyph.selected rect { stroke: var(--accent); fill: var(--accent-soft); }
  .glyph.selected text { fill: var(--accent-2); }
  .ghost { fill: none; stroke: var(--accent); stroke-width: 1.6; stroke-dasharray: 5 3; }
  .ghost.construction { stroke: var(--sketch-construction); stroke-width: 1; }
  .ghost-point { fill: var(--accent); }
  .removed { fill: none; stroke: var(--danger); stroke-width: 3; stroke-dasharray: 4 3; }
  .inference { stroke: var(--ink-3); stroke-width: 1; stroke-dasharray: 2 4; }
  .snap { fill: none; stroke: var(--ok); stroke-width: 2; }
  .snap.grid-snap { stroke-width: 1.5; }
  .box { fill: var(--sketch-soft); stroke: var(--sketch); stroke-width: 1; }
  .box.crossing { fill: rgba(47, 143, 91, .08); stroke: var(--ok); stroke-dasharray: 5 3; }
  .hud { position: absolute; display: flex; flex-direction: column; gap: 4px; pointer-events: auto; }
  .hud-field { display: grid; grid-template-columns: minmax(100px, 1fr) 70px 22px; align-items: center; min-height: 40px; padding: 0 8px; border-radius: 8px; border: 1px solid var(--line-2); background: color-mix(in srgb, var(--panel) 92%, transparent); box-shadow: var(--shadow); font-size: var(--t-sm); cursor: pointer; text-align: left; }
  .hud-field.active { border-color: var(--sketch); box-shadow: 0 0 0 2px color-mix(in srgb, var(--sketch) 30%, transparent); }
  .hud-field.locked .hud-value { color: var(--accent-2); }
  .hud-label { color: var(--ink-3); }
  .hud-value { font-weight: 700; font-variant-numeric: tabular-nums; text-align: right; }
  .hud-unit { color: var(--ink-3); padding-left: 4px; }
  .caret { display: inline-block; width: 1px; height: 1em; margin-left: 1px; vertical-align: -2px; background: currentColor; animation: blink 1s steps(1) infinite; }
  @keyframes blink { 50% { opacity: 0; } }
  .sketch-status { position: absolute; left: 0; right: 0; bottom: 0; display: flex; justify-content: space-between; gap: 12px; padding: 6px 12px; font-size: var(--t-sm); color: var(--ink-2); background: color-mix(in srgb, var(--panel) 85%, transparent); border-top: 1px solid var(--line); pointer-events: none; }
  .sketch-coords { font-variant-numeric: tabular-nums; white-space: nowrap; }
  .sketch-coords b { color: var(--ok); margin-right: 10px; }
  .sketch-side { gap: 14px; }
  .sk-name { display: flex; flex-direction: column; gap: 6px; font-size: var(--t-sm); color: var(--ink-3); font-weight: 600; }
  .sk-name-field { min-height: 44px; padding: 0 12px; border-radius: 10px; border: 1px solid var(--line); background: var(--panel-2); font-weight: 600; color: var(--ink); text-align: left; cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sk-status { margin: 6px 0 0; font-size: var(--t-sm); color: var(--sketch); font-weight: 600; }
  .sk-status.done { color: var(--ok); }
  .sk-facts dl, .sk-keys dl { display: grid; grid-template-columns: auto 1fr; gap: 6px 12px; margin: 8px 0 0; font-size: var(--t-sm); }
  .sk-facts dt { color: var(--ink-3); }
  .sk-facts dt.kind { color: var(--ink); font-weight: 700; }
  .sk-facts dd, .sk-keys dd { margin: 0; text-align: right; font-variant-numeric: tabular-nums; }
  .sk-small { min-height: 40px; margin-top: 6px; }
  .sk-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-top: 8px; font-size: var(--t-sm); }
  .sk-row .seg button { min-height: 36px; padding: 0 10px; font-size: var(--t-sm); }
  .sk-value { min-height: 36px; padding: 0 10px; border-radius: 8px; border: 1px solid var(--line); background: var(--panel-2); color: var(--ink); font-weight: 600; cursor: pointer; }
  .sk-check { display: flex; align-items: center; gap: 8px; min-height: 36px; font-size: var(--t-sm); cursor: pointer; }
  .sk-keys dt { font-weight: 600; }
  .sk-keys dd { text-align: left; color: var(--ink-2); }
  .sk-actions { display: flex; flex-direction: column; gap: 8px; margin-top: auto; }
</style>
