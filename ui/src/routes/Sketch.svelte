<script lang="ts">
  // A 2D sketch drawn as in Fusion: pick a tool, click points, type a
  // value to lock it (Tab moves on, Enter places), and Escape steps back.
  // The finished sketch becomes a library part through the DXF importer.
  import { onMount } from 'svelte';
  import { api } from '../api/client';
  import { server } from '../stores/server.svelte';
  import { ui } from '../stores/ui.svelte';
  import { osk } from '../lib/osk.svelte';
  import { explain } from '../lib/format';
  import { displayNumber, toSource, unitLabel } from '../lib/units.svelte';
  import { sketchStore as store } from '../sketch/store.svelte';
  import { clone, merge, point, type Id, type Sketch } from '../sketch/model';
  import { angleOf, dist, polar, shapeOf, sketchBounds, sweep, insideBox, touchesBox, type Shape, type Vec } from '../sketch/geometry';
  import { gridStep, snap, type Snap } from '../sketch/snap';
  import { type Field, type Locked, type ToolId } from '../sketch/tools';
  import { chain, hit, hitPoint, originsOf, repairArcs, translate } from '../sketch/edit';
  import { hasCut, toDxf } from '../sketch/dxf';
  import { evaluate } from '../sketch/expression';

  type ToolButton = { id: ToolId | null; label: string; key?: string; icon: string };
  const TOOLS: ToolButton[] = [
    { id: null, label: 'Select', key: 'Esc', icon: 'M6 3l12 9-5.5 1.2L16 20l-2.6 1.2-2.9-6.7L6 18z' },
    { id: 'line', label: 'Line', key: 'L', icon: 'M5 19L19 5M5 19m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0M19 5m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0' },
    { id: 'rect2', label: '2-point rectangle', key: 'R', icon: 'M4 6h16v12H4zM4 18m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0M20 6m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0' },
    { id: 'rectc', label: 'Center rectangle', icon: 'M4 6h16v12H4zM4 6l16 12M20 6L4 18' },
    { id: 'circle', label: 'Center diameter circle', key: 'C', icon: 'M12 4a8 8 0 1 0 0 16a8 8 0 1 0 0-16M12 12h8M12 12m-1.2 0a1.2 1.2 0 1 0 2.4 0a1.2 1.2 0 1 0-2.4 0' },
    { id: 'circle2', label: '2-point circle', icon: 'M12 4a8 8 0 1 0 0 16a8 8 0 1 0 0-16M4 12h16' },
    { id: 'arc3', label: '3-point arc', key: 'A', icon: 'M4 18A9 9 0 0 1 20 18M4 18m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0M20 18m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0M12 9m-1.6 0a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0-3.2 0' },
    { id: 'arcc', label: 'Center point arc', icon: 'M20 15A8 8 0 0 0 7 8.5M12 15h8M12 15L7 8.5M12 15m-1.2 0a1.2 1.2 0 1 0 2.4 0a1.2 1.2 0 1 0-2.4 0' },
    { id: 'polygon', label: 'Inscribed polygon', key: 'P', icon: 'M12 3l7.8 4.5v9L12 21l-7.8-4.5v-9z' },
    { id: 'slot', label: 'Center to center slot', icon: 'M8 7h8a5 5 0 0 1 0 10H8A5 5 0 0 1 8 7M8 12h8' },
  ];
  const SHORTCUTS: Record<string, ToolId> = { l: 'line', r: 'rect2', c: 'circle', a: 'arc3', p: 'polygon' };

  // ---- View: millimetres with Y up, drawn in screen pixels -------------------------------------
  let w = $state(800);
  let h = $state(600);
  let view = $state(store.view ?? { cx: 100, cy: 60, s: 3 });
  $effect(() => { store.view = view; });
  let svg: SVGSVGElement | undefined = $state();
  const toScreen = (p: Vec): Vec => ({ x: (p.x - view.cx) * view.s + w / 2, y: h / 2 - (p.y - view.cy) * view.s });
  const toModel = (x: number, y: number): Vec => ({ x: view.cx + (x - w / 2) / view.s, y: view.cy - (y - h / 2) / view.s });

  // ---- Cursor, snapping and the tool's heads-up fields -------------------------------------------
  let cursor = $state.raw<Vec | null>(null);
  let touch = $state(false);
  let locked = $state<Locked>({});
  let editing = $state<{ key: string; text: string } | null>(null);
  let active = $state(0);
  let dragSnap = $state.raw<Snap | null>(null);

  const reach = $derived((touch ? 16 : 9) / view.s);
  const minor = $derived(gridStep(1 / view.s, 10));
  const major = $derived(minor * (String(minor).replace(/[0.]/g, '').startsWith('2') ? 5 : String(minor).replace(/[0.]/g, '').startsWith('5') ? 2 : 5));
  const tool = $derived(store.tool);
  const snapped = $derived.by((): Snap | null => {
    void store.toolRevision;
    if (!cursor || !tool) return null;
    return snap(store.sketch, cursor, { reach, grid: store.gridSnap ? minor : 0, anchor: tool.anchor() });
  });
  const resolved = $derived.by((): Snap | null => {
    void store.toolRevision;
    return tool && snapped ? tool.resolve(snapped, locked) : null;
  });
  const fields = $derived.by((): Field[] => {
    void store.toolRevision;
    return tool && resolved ? tool.fields(resolved) : [];
  });
  const ghost = $derived.by(() => {
    void store.toolRevision;
    return tool && resolved ? tool.ghost(resolved, locked) : null;
  });
  const anchor = $derived.by(() => { void store.toolRevision; return tool?.anchor() ?? null; });
  const hint = $derived.by(() => { void store.toolRevision; return tool ? tool.hint() : 'Click to select · drag a box · double click selects a connected profile'; });

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
    if ((br.x - tl.x) / minor > 400) return lines;
    for (let x = Math.ceil(tl.x / minor) * minor; x <= br.x; x += minor) {
      const X = toScreen({ x, y: 0 }).x;
      lines.push({ d: `M${X} 0V${h}`, major: Math.abs(x / major - Math.round(x / major)) < 1e-6 });
    }
    for (let y = Math.ceil(br.y / minor) * minor; y <= tl.y; y += minor) {
      const Y = toScreen({ x: 0, y }).y;
      lines.push({ d: `M0 ${Y}H${w}`, major: Math.abs(y / major - Math.round(y / major)) < 1e-6 });
    }
    return lines;
  });
  const origin = $derived(toScreen({ x: 0, y: 0 }));

  const drawn = $derived(store.sketch.entities.map((e) => ({ id: e.id, construction: !!e.construction, d: pathOf(shapeOf(store.sketch, e)) })));
  const centers = $derived(new Set(store.sketch.entities.flatMap((e) => (e.kind === 'line' ? [] : [e.c]))));
  const handles = $derived(store.sketch.points.map((p) => ({ id: p.id, center: centers.has(p.id), ...toScreen(p) })));

  // ---- Fitting the view ----------------------------------------------------------------------
  function fit(): void {
    const b = sketchBounds(store.sketch);
    if (!b) { view = { cx: 100, cy: 60, s: Math.min(w / 260, h / 160) }; return; }
    const bw = Math.max(b.maxX - b.minX, 10), bh = Math.max(b.maxY - b.minY, 10);
    view = { cx: (b.minX + b.maxX) / 2, cy: (b.minY + b.maxY) / 2, s: Math.min(w / (bw * 1.25), h / (bh * 1.25)) };
  }
  onMount(() => { if (!store.view) requestAnimationFrame(fit); });

  // ---- Pointer input ---------------------------------------------------------------------------
  type Drag =
    | { kind: 'pan'; x: number; y: number; cx: number; cy: number }
    | { kind: 'press'; x: number; y: number; start: Vec; moved: boolean; mode: 'tool' | 'point' | 'move' | 'radius' | 'box'; target: Id | null; original: Sketch; origins: Map<Id, Vec>; shift: boolean }
    | { kind: 'pinch'; d: number; mid: Vec; view: { cx: number; cy: number; s: number }; model: Vec };
  let drag: Drag | null = null;
  let box = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const pointers = new Map<number, { x: number; y: number }>();

  function local(e: PointerEvent | WheelEvent | MouseEvent): { x: number; y: number } {
    const r = svg!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
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
    if (tool) { drag = { ...base, mode: 'tool', target: null }; return; }
    const p = hitPoint(original, m, reach);
    if (p !== null) { drag = { ...base, mode: 'point', target: p, origins: originsOf(original, []).set(p, { ...point(original, p) }) }; return; }
    const id = hit(original, m, reach);
    if (id === null) { drag = { ...base, mode: 'box', target: null }; return; }
    const entity = original.entities.find((x) => x.id === id)!;
    if (entity.kind === 'circle' && !store.selection.has(id)) { drag = { ...base, mode: 'radius', target: id }; return; }
    const moving = store.selection.has(id) ? [...store.selection] : [id];
    drag = { ...base, mode: 'move', target: id, origins: originsOf(original, moving) };
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
    if (drag?.kind !== 'press') return;
    const threshold = touch ? 12 : 4;
    if (!drag.moved && Math.hypot(at.x - drag.x, at.y - drag.y) < threshold) return;
    drag.moved = true;
    if (drag.mode === 'tool') return;
    if (drag.mode === 'box') { box = { x0: drag.x, y0: drag.y, x1: at.x, y1: at.y }; return; }
    const next = clone(drag.original);
    if (drag.mode === 'radius') {
      const target = drag.target;
      const e2 = next.entities.find((x) => x.id === target);
      if (e2?.kind === 'circle') e2.r = Math.max(1e-3, dist(point(next, e2.c), store.gridSnap ? snap(next, m, { reach, grid: minor }) : m));
      store.preview(next);
      return;
    }
    let delta = { x: m.x - drag.start.x, y: m.y - drag.start.y };
    if (drag.mode === 'point' && drag.target !== null) {
      const from = drag.origins.get(drag.target)!;
      dragSnap = snap(drag.original, { x: from.x + delta.x, y: from.y + delta.y }, { reach, grid: store.gridSnap ? minor : 0, anchor: from, ignorePoints: new Set([drag.target]) });
      delta = { x: dragSnap.x - from.x, y: dragSnap.y - from.y };
    } else if (store.gridSnap) {
      delta = { x: Math.round(delta.x / minor) * minor, y: Math.round(delta.y / minor) * minor };
    }
    translate(next, drag.origins, delta);
    repairArcs(next, drag.original);
    store.preview(next);
  }

  function up(e: PointerEvent): void {
    pointers.delete(e.pointerId);
    const d = drag;
    if (d?.kind === 'pinch') { if (pointers.size < 2) drag = null; return; }
    drag = null;
    if (!d || d.kind !== 'press') { dragSnap = null; return; }
    if (d.mode === 'tool') { if (!d.moved || touch) place(); return; }
    if (!d.moved) {
      if (d.mode === 'box') { if (!d.shift) store.select([]); return; }
      const id = d.mode === 'point' ? hit(store.sketch, d.start, reach) : d.target;
      if (id !== null) store.select([id], d.shift ? 'toggle' : 'replace');
      return;
    }
    if (d.mode === 'box' && box) {
      const a = toModel(box.x0, box.y0), b = toModel(box.x1, box.y1);
      const area = { minX: Math.min(a.x, b.x), maxX: Math.max(a.x, b.x), minY: Math.min(a.y, b.y), maxY: Math.max(a.y, b.y) };
      // Left to right selects what lies inside; right to left whatever the box touches, as in Fusion.
      const window = box.x1 >= box.x0;
      const ids = store.sketch.entities.filter((x) => (window ? insideBox : touchesBox)(shapeOf(store.sketch, x), area)).map((x) => x.id);
      store.select(ids, d.shift ? 'add' : 'replace');
      box = null;
      return;
    }
    const finished = clone(store.sketch);
    if (d.mode === 'point' && d.target !== null && dragSnap?.point !== undefined) merge(finished, d.target, dragSnap.point);
    dragSnap = null;
    store.commit(finished, d.original);
  }

  function cancelPointer(e: PointerEvent): void {
    pointers.delete(e.pointerId);
    if (drag?.kind === 'press' && drag.moved && drag.mode !== 'tool' && drag.mode !== 'box') store.preview(drag.original);
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
    if (tool) return;
    const at = local(e);
    const id = hit(store.sketch, toModel(at.x, at.y), reach);
    if (id !== null) store.select(chain(store.sketch, id), e.shiftKey ? 'add' : 'replace');
  }

  // ---- Placing points and the heads-up fields ----------------------------------------------------
  function place(): void {
    if (!tool) return;
    commitEditing();
    const at = cursor ? tool.resolve(snap(store.sketch, cursor, { reach, grid: store.gridSnap ? minor : 0, anchor: tool.anchor() }), locked) : null;
    if (!at) return;
    const draft = clone(store.sketch);
    const outcome = tool.click(draft, at, locked);
    if (outcome === 'ignored') return;
    if (draft.next !== store.sketch.next) store.commit(draft);
    locked = {};
    editing = null;
    active = 0;
    store.toolRevision++;
  }

  function parsed(field: Field, text: string): number | null {
    const v = evaluate(text);
    if (v === null) return null;
    if (field.kind === 'length') return v > 0 ? toSource(v, 'mm') : null;
    if (field.kind === 'count') return v >= 3 ? Math.round(v) : null;
    return v;
  }

  function commitEditing(): void {
    if (!editing) return;
    const field = fields.find((f) => f.key === editing!.key);
    const v = field ? parsed(field, editing.text) : null;
    if (field && v !== null) locked = { ...locked, [field.key]: v };
    editing = null;
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
      const ok = field.kind === 'length' ? v > 0 : field.kind === 'count' ? v >= 3 : true;
      if (ok) locked = { ...locked, [field.key]: field.kind === 'count' ? Math.round(v) : v };
    });
  }

  // ---- Keyboard ---------------------------------------------------------------------------------
  let spaceHeld = false;

  function escape(): void {
    if (editing) { editing = null; return; }
    if (Object.keys(locked).length) { locked = {}; return; }
    if (tool?.busy) { tool.reset(); store.toolRevision++; return; }
    if (tool) { store.use(null); return; }
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
    if (mod || e.altKey) return;
    if (e.key === 'Escape') { e.preventDefault(); escape(); return; }
    if (fields.length && (/^[0-9.,+\-*/()]$/.test(e.key) || (editing && e.key === 'Backspace'))) {
      e.preventDefault();
      const field = fields[Math.min(active, fields.length - 1)]!;
      const text = editing?.key === field.key ? editing.text : '';
      editing = { key: field.key, text: e.key === 'Backspace' ? text.slice(0, -1) : text + e.key };
      return;
    }
    if (e.key === 'Tab' && tool) {
      e.preventDefault();
      if (!fields.length) return;
      commitEditing();
      active = (Math.min(active, fields.length - 1) + (e.shiftKey ? fields.length - 1 : 1)) % fields.length;
      return;
    }
    if (e.key === 'Enter' && tool) { e.preventDefault(); place(); return; }
    if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); store.deleteSelection(); return; }
    const k = e.key.toLowerCase();
    if (SHORTCUTS[k]) { store.use(SHORTCUTS[k]!); locked = {}; editing = null; active = 0; return; }
    if (k === 'x') { store.toggleConstruction(); return; }
    if (k === 'f') { fit(); return; }
    if (k === 'g') { store.gridSnap = !store.gridSnap; }
  }
  function keyUp(e: KeyboardEvent): void { if (e.key === ' ') spaceHeld = false; }

  function choose(id: ToolId | null): void {
    store.use(id);
    locked = {};
    editing = null;
    active = 0;
  }

  // ---- Leaving the sketch ------------------------------------------------------------------------
  let sending = $state(false);
  const fileName = $derived(`${store.name.replace(/[\\/:*?"<>|]/g, ' ').trim() || 'Sketch'}.dxf`);

  async function send(): Promise<void> {
    if (sending || !hasCut(store.sketch)) return;
    sending = true;
    try {
      const bytes = new TextEncoder().encode(toDxf(store.sketch));
      const { id, warnings } = await api.importPart(fileName, bytes.buffer as ArrayBuffer);
      ui.selected = id;
      if (server.doc?.draft) await api.addParts([id]);
      else await api.openPart(id);
      ui.tab = 'setup';
      ui.say(warnings?.length ? `Sketch added as a part · ${warnings.join(' · ')}` : 'Sketch added as a part. Choose a material to prepare it for cutting.');
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

  // ---- What the side panel reports about the selection --------------------------------------------
  const selected = $derived(store.sketch.entities.filter((e) => store.selection.has(e.id)));
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

  const snapLabel: Record<string, string> = { end: 'Endpoint', center: 'Center', mid: 'Midpoint', intersection: 'Intersection', on: 'On curve', horizontal: 'Horizontal', vertical: 'Vertical', grid: 'Grid', free: '' };
  const marker = $derived(dragSnap ?? resolved);
  const coords = $derived(resolved ?? cursor);
</script>

<svelte:window onkeydown={key} onkeyup={keyUp} />

<section class="panel main sketch-main">
  <div class="sketch-bar" role="toolbar" aria-label="Sketch tools">
    {#each TOOLS as t (t.label)}
      <button class="sk-tool" class:on={(tool?.id ?? null) === t.id} title={t.key ? `${t.label} (${t.key})` : t.label} aria-label={t.label} onclick={() => choose(t.id)}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d={t.icon} /></svg>
      </button>
    {/each}
    <span class="sk-sep"></span>
    <button class="sk-tool" title="Construction (X)" aria-label="Toggle construction" disabled={!store.selection.size} onclick={() => store.toggleConstruction()}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20L20 4" stroke-dasharray="3 3" /></svg>
    </button>
    <button class="sk-tool" title="Delete (Del)" aria-label="Delete" disabled={!store.selection.size} onclick={() => store.deleteSelection()}><i class="ic ic-trash"></i></button>
    <span class="sk-sep"></span>
    <button class="sk-tool" title="Undo (Ctrl+Z)" aria-label="Undo" disabled={!store.canUndo} onclick={() => store.undo()}><i class="ic ic-undo"></i></button>
    <button class="sk-tool" title="Redo (Ctrl+Y)" aria-label="Redo" disabled={!store.canRedo} onclick={() => store.redo()}><i class="ic ic-redo"></i></button>
    <span class="sk-sep"></span>
    <button class="sk-tool" title="Fit (F)" aria-label="Fit" onclick={fit}><i class="ic ic-fit"></i></button>
    <button class="sk-tool wide" class:on={store.gridSnap} title="Snap to grid (G)" onclick={() => (store.gridSnap = !store.gridSnap)}>Grid snap</button>
  </div>

  <div class="sketch-canvas" bind:clientWidth={w} bind:clientHeight={h}>
    <svg bind:this={svg} width={w} height={h} class:drawing={!!tool} role="application" aria-label="Sketch canvas"
      onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={cancelPointer}
      onpointerleave={() => { if (!drag) cursor = null; }} onwheel={wheel} ondblclick={doubleClick}
      oncontextmenu={(e) => { e.preventDefault(); escape(); }}>
      <g class="grid">{#each gridLines as g, i (i)}<path d={g.d} class:major={g.major} />{/each}</g>
      <path class="axis-x" d={`M0 ${origin.y}H${w}`} />
      <path class="axis-y" d={`M${origin.x} 0V${h}`} />
      {#each drawn as e (e.id)}
        <path d={e.d} class="curve" class:construction={e.construction} class:selected={store.selection.has(e.id)} />
      {/each}
      {#each handles as p (p.id)}
        {#if p.center}<path class="handle center" d={`M${p.x - 3} ${p.y}h6M${p.x} ${p.y - 3}v6`} />{:else}<rect class="handle" x={p.x - 2.5} y={p.y - 2.5} width="5" height="5" />{/if}
      {/each}
      {#if ghost}
        {#each ghost.construction as s, i (i)}<path d={pathOf(s)} class="ghost construction" />{/each}
        {#each ghost.shapes as s, i (i)}<path d={pathOf(s)} class="ghost" />{/each}
        {#each ghost.points as p, i (i)}{@const q = toScreen(p)}<circle cx={q.x} cy={q.y} r="2.5" class="ghost-point" />{/each}
      {/if}
      {#if anchor && resolved && (resolved.kind === 'horizontal' || resolved.kind === 'vertical')}
        {@const a = toScreen(anchor)}{@const b = toScreen(resolved)}<path class="inference" d={`M${a.x} ${a.y}L${b.x} ${b.y}`} />
      {/if}
      {#if marker && marker.kind !== 'free' && marker.kind !== 'grid'}
        {@const q = toScreen(marker)}
        {#if marker.kind === 'end'}<rect class="snap" x={q.x - 6} y={q.y - 6} width="12" height="12" />
        {:else if marker.kind === 'center'}<circle class="snap" cx={q.x} cy={q.y} r="7" />
        {:else if marker.kind === 'mid'}<path class="snap" d={`M${q.x} ${q.y - 7}L${q.x + 7} ${q.y + 5}H${q.x - 7}Z`} />
        {:else if marker.kind === 'intersection'}<path class="snap" d={`M${q.x - 6} ${q.y - 6}L${q.x + 6} ${q.y + 6}M${q.x + 6} ${q.y - 6}L${q.x - 6} ${q.y + 6}`} />
        {:else if marker.kind === 'on'}<path class="snap" d={`M${q.x} ${q.y - 7}L${q.x + 7} ${q.y}L${q.x} ${q.y + 7}L${q.x - 7} ${q.y}Z`} />{/if}
      {/if}
      {#if box}
        <rect class="box" class:crossing={box.x1 < box.x0} x={Math.min(box.x0, box.x1)} y={Math.min(box.y0, box.y1)} width={Math.abs(box.x1 - box.x0)} height={Math.abs(box.y1 - box.y0)} />
      {/if}
    </svg>

    {#if fields.length && resolved}
      {@const q = toScreen(resolved)}
      <div class="hud" style:left={`${Math.min(q.x + 18, w - 170)}px`} style:top={`${Math.max(8, Math.min(q.y + 18, h - 44 * fields.length - 40))}px`}>
        {#each fields as f, i (f.key)}
          <button class="hud-field" class:active={i === Math.min(active, fields.length - 1)} class:locked={f.key in locked || editing?.key === f.key} data-numpad onclick={() => tapField(f, i)}>
            <span class="hud-label">{f.label}</span><span class="hud-value">{shown(f)}{#if editing?.key === f.key}<span class="caret"></span>{/if}</span><span class="hud-unit">{suffix(f)}</span>
          </button>
        {/each}
      </div>
    {/if}

    <div class="sketch-status">
      <span>{hint}</span>
      <span class="sketch-coords">{#if marker && snapLabel[marker.kind]}<b>{snapLabel[marker.kind]}</b>{/if}{#if coords}X {displayNumber(coords.x, 'mm')} · Y {displayNumber(coords.y, 'mm')} {unitLabel('mm')}{/if}</span>
    </div>
  </div>
</section>

<aside class="panel side sketch-side">
  <div class="sk-name"><span>Name</span><button class="sk-name-field" onclick={() => osk.text('Sketch name', store.name, (v) => store.rename(v))}>{store.name}</button></div>

  <div class="sk-facts">
    <h3>Selection</h3>
    {#if !selected.length}<p class="muted">{store.sketch.entities.length} curves. Nothing selected.</p>
    {:else if facts.length}
      <dl>{#each facts as [k, v] (k)}{#if v}<dt>{k}</dt><dd>{v}</dd>{:else}<dt class="kind">{k}</dt><dd></dd>{/if}{/each}</dl>
    {:else}<p>{selected.length} curves selected</p>{/if}
  </div>


  <details class="sk-keys">
    <summary>Keyboard</summary>
    <dl>
      <dt>L R C A P</dt><dd>Line, rectangle, circle, arc, polygon</dd>
      <dt>Type a number</dt><dd>Locks the highlighted value; 50/2 works</dd>
      <dt>Tab</dt><dd>Next value</dd>
      <dt>Enter</dt><dd>Place the point</dd>
      <dt>Esc · right click</dt><dd>Clear values, end the shape, then Select</dd>
      <dt>X</dt><dd>Construction on or off</dd>
      <dt>Del</dt><dd>Delete selection</dd>
      <dt>F · G</dt><dd>Fit · grid snap</dd>
      <dt>Wheel · middle drag</dt><dd>Zoom · pan (also Space+drag, two fingers)</dd>
    </dl>
  </details>

  <div class="sk-actions">
    <button class="btn btn-primary lg block" disabled={sending || !hasCut(store.sketch)} onclick={send}>{sending ? 'Adding…' : 'Add to parts'}</button>
    <button class="btn btn-ghost block" disabled={!hasCut(store.sketch)} onclick={download}>Download DXF</button>
    <button class="btn btn-ghost block" disabled={!store.sketch.entities.length} onclick={() => store.clear()}>New sketch</button>
  </div>
</aside>

<style>
  .sketch-main { position: relative; }
  .sketch-bar { display: flex; flex-wrap: wrap; gap: 4px; padding: 8px; border-bottom: 1px solid var(--line); background: var(--panel-2); }
  .sk-tool { min-width: 44px; height: 44px; border-radius: 10px; border: 1px solid transparent; background: transparent; display: inline-flex; align-items: center; justify-content: center; cursor: pointer; color: var(--ink-2); }
  .sk-tool.wide { padding: 0 12px; font-weight: 600; font-size: var(--t-sm); }
  .sk-tool:hover:not(:disabled) { background: var(--panel); border-color: var(--line); }
  .sk-tool.on { background: var(--accent-soft); color: var(--accent-2); border-color: transparent; }
  .sk-tool:disabled { opacity: .35; cursor: not-allowed; }
  .sk-tool svg { width: 24px; height: 24px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  .sk-sep { width: 1px; margin: 6px 4px; background: var(--line-2); }
  .sketch-canvas { position: relative; flex: 1; min-height: 0; overflow: hidden; background: var(--panel); }
  .sketch-canvas svg { display: block; touch-action: none; cursor: default; }
  .sketch-canvas svg.drawing { cursor: crosshair; }
  .grid path { stroke: var(--grid); stroke-width: 1; }
  .grid path.major { stroke: var(--grid-strong); }
  .axis-x { stroke: #d9534f; stroke-width: 1; opacity: .7; }
  .axis-y { stroke: #3f9a5a; stroke-width: 1; opacity: .7; }
  .curve { fill: none; stroke: var(--sketch); stroke-width: 1.8; stroke-linecap: round; }
  .curve.construction { stroke: var(--sketch-construction); stroke-dasharray: 6 4; stroke-width: 1.3; }
  .curve.selected { stroke: var(--accent); stroke-width: 3; }
  .handle { fill: var(--panel); stroke: var(--sketch); stroke-width: 1.2; }
  .handle.center { fill: none; }
  .ghost { fill: none; stroke: var(--accent); stroke-width: 1.6; stroke-dasharray: 5 3; }
  .ghost.construction { stroke: var(--sketch-construction); stroke-width: 1; }
  .ghost-point { fill: var(--accent); }
  .inference { stroke: var(--ink-3); stroke-width: 1; stroke-dasharray: 2 4; }
  .snap { fill: none; stroke: var(--ok); stroke-width: 2; }
  .box { fill: var(--sketch-soft); stroke: var(--sketch); stroke-width: 1; }
  .box.crossing { fill: rgba(47, 143, 91, .08); stroke: var(--ok); stroke-dasharray: 5 3; }
  .hud { position: absolute; display: flex; flex-direction: column; gap: 4px; pointer-events: auto; }
  .hud-field { display: grid; grid-template-columns: 64px 70px 22px; align-items: center; min-height: 40px; padding: 0 8px; border-radius: 8px; border: 1px solid var(--line-2); background: color-mix(in srgb, var(--panel) 92%, transparent); box-shadow: var(--shadow); font-size: var(--t-sm); cursor: pointer; text-align: left; }
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
  .sketch-side { gap: 16px; }
  .sk-name { display: flex; flex-direction: column; gap: 6px; font-size: var(--t-sm); color: var(--ink-3); font-weight: 600; }
  .sk-name-field { min-height: 44px; padding: 0 12px; border-radius: 10px; border: 1px solid var(--line); background: var(--panel-2); font-weight: 600; color: var(--ink); text-align: left; cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sk-facts dl, .sk-keys dl { display: grid; grid-template-columns: auto 1fr; gap: 6px 12px; margin: 8px 0 0; font-size: var(--t-sm); }
  .sk-facts dt { color: var(--ink-3); }
  .sk-facts dt.kind { color: var(--ink); font-weight: 700; }
  .sk-facts dd, .sk-keys dd { margin: 0; text-align: right; font-variant-numeric: tabular-nums; }
  .sk-keys dt { font-weight: 600; }
  .sk-keys dd { text-align: left; color: var(--ink-2); }
  .sk-actions { display: flex; flex-direction: column; gap: 8px; margin-top: auto; }
</style>
