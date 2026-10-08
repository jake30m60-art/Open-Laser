// The sketch's constraints, solved by FreeCAD's PlaneGCS (WebAssembly).
// Points become solver points, curves refer to them, and every relation
// and dimension becomes one or more solver constraints. Dragging adds
// temporary constraints that pull points towards the cursor as far as the
// real constraints allow, as FreeCAD's and Fusion's sketchers do.
import { make_gcs_wrapper, SolveStatus, type GcsWrapper, type SketchPrimitive } from '@salusoft89/planegcs';
import { resizedFillets } from './fillet-resize';
import { angleOf, dist, sweep } from './geometry';
import { clone, constraintRefs, endsOf, point, pointsOf, type Constraint, type Ellipse, type EllipseArc, type Entity, type Id, type Sketch } from './model';
import { arcParams, ellipseFrame } from './curves';

let wrapper: GcsWrapper | null = null;
let loading: Promise<void> | null = null;
let wasmUrl: string | undefined;

/** Loads the solver once; `wasm` locates its WebAssembly in a browser build. */
export function initSolver(wasm?: string): Promise<void> {
  wasmUrl ??= wasm;
  loading ??= make_gcs_wrapper(wasmUrl).then((w) => { wrapper = w; });
  return loading;
}

/** A solver that failed inside its WebAssembly cannot be used again: load a fresh one. */
function restart(): void {
  wrapper = null;
  loading = null;
  void initSolver().catch(() => undefined);
}

export const solverReady = (): boolean => wrapper !== null;

/** A point held towards a position while the rest settles. */
export interface Pull { point: Id; x: number; y: number }

export interface Solved {
  /** A geometric edit that cannot preserve its reference contour. */
  reason?: string;
  /** Whether every constraint holds. */
  ok: boolean;
  /** The sketch as solved; the input unchanged when solving failed. */
  sketch: Sketch;
  /** Constraints that contradict others or say again what others say. */
  conflicting: Id[];
  redundant: Id[];
  /** Degrees of freedom left: zero is a fully defined sketch. */
  dof: number;
}

const P = (id: Id) => `p${id}`;
const E = (id: Id) => `e${id}`;

/** The id of the sketch constraint a solver constraint came from. */
function owner(solverId: string): Id | null {
  const m = /^c(\d+)/.exec(solverId);
  return m ? Number(m[1]) : null;
}

function entityOf(sketch: Sketch, id: Id): Entity | undefined {
  return sketch.entities.find((e) => e.id === id);
}

/** The signed angle in radians from line `a`'s direction to line `b`'s. */
export function lineAngle(sketch: Sketch, a: Id, b: Id): number {
  const la = entityOf(sketch, a), lb = entityOf(sketch, b);
  if (la?.kind !== 'line' || lb?.kind !== 'line') return 0;
  const da = angleOf(point(sketch, la.a), point(sketch, la.b));
  const db = angleOf(point(sketch, lb.a), point(sketch, lb.b));
  let d = db - da;
  while (d > Math.PI) d -= 2 * Math.PI;
  while (d <= -Math.PI) d += 2 * Math.PI;
  return d;
}

/** The end two lines or arcs share, if any. */
function sharedEnd(sketch: Sketch, a: Id, b: Id): Id | null {
  const ends = (id: Id): Id[] => { const e = entityOf(sketch, id); return (e && endsOf(e)) ?? []; };
  const theirs = ends(b);
  return ends(a).find((p) => theirs.includes(p)) ?? null;
}

/** A curve's normal at a point in FreeCAD's convention: a line's direction
 *  turned a quarter counter-clockwise, a circle's towards its centre. */
function normalAt(sketch: Sketch, id: Id, at: Id): { x: number; y: number } {
  const e = entityOf(sketch, id)!;
  const p = point(sketch, at);
  if (e.kind === 'line') { const a = point(sketch, e.a), b = point(sketch, e.b); return { x: -(b.y - a.y), y: b.x - a.x }; }
  if (e.kind === 'spline') return { x: 0, y: 0 };
  const c = point(sketch, e.c);
  return { x: c.x - p.x, y: c.y - p.y };
}

/** 0 or π, whichever the curves are nearer to at their shared end. */
function tangentAngle(sketch: Sketch, a: Id, b: Id, at: Id): number {
  const n1 = normalAt(sketch, a, at), n2 = normalAt(sketch, b, at);
  const angle = Math.atan2(n1.x * n2.y - n1.y * n2.x, n1.x * n2.x + n1.y * n2.y);
  return Math.abs(angle) < Math.PI / 2 ? 0 : Math.PI;
}

/** The solver constraints for one sketch constraint; none when its curves
 *  are of kinds it does not relate. */
function translate(sketch: Sketch, c: Constraint): SketchPrimitive[] {
  const id = `c${c.id}`;
  if ('driven' in c && c.driven) return [];
  const kind = (e: Id) => entityOf(sketch, e)?.kind;
  switch (c.kind) {
    case 'horizontal': return [{ id, type: 'horizontal_l', l_id: E(c.line) }];
    case 'vertical': return [{ id, type: 'vertical_l', l_id: E(c.line) }];
    case 'parallel': return [{ id, type: 'parallel', l1_id: E(c.a), l2_id: E(c.b) }];
    case 'perpendicular': return [{ id, type: 'perpendicular_ll', l1_id: E(c.a), l2_id: E(c.b) }];
    case 'collinear': {
      const b = entityOf(sketch, c.b);
      if (b?.kind !== 'line') return [];
      return [
        { id, type: 'parallel', l1_id: E(c.a), l2_id: E(c.b) },
        { id: `${id}_on`, type: 'point_on_line_pl', p_id: P(b.a), l_id: E(c.a) },
      ];
    }
    case 'equal': {
      const [ka, kb] = [kind(c.a), kind(c.b)];
      if (ka === 'line' && kb === 'line') return [{ id, type: 'equal_length', l1_id: E(c.a), l2_id: E(c.b) }];
      if (ka === 'circle' && kb === 'circle') return [{ id, type: 'equal_radius_cc', c1_id: E(c.a), c2_id: E(c.b) }];
      if (ka === 'arc' && kb === 'arc') return [{ id, type: 'equal_radius_aa', a1_id: E(c.a), a2_id: E(c.b) }];
      if (ka === 'circle' && kb === 'arc') return [{ id, type: 'equal_radius_ca', c1_id: E(c.a), a2_id: E(c.b) }];
      if (ka === 'arc' && kb === 'circle') return [{ id, type: 'equal_radius_ca', c1_id: E(c.b), a2_id: E(c.a) }];
      return [];
    }
    case 'tangent': {
      const [ka, kb] = [kind(c.a), kind(c.b)];
      if ([ka, kb].some((k) => k === 'ellipse' || k === 'ellipse-arc' || k === 'spline')) return [];
      let joint = sharedEnd(sketch, c.a, c.b);
      // A line meeting a circle at an on-curve endpoint is tangent there.
      // Angle-at-contact avoids the degenerate combination of distance-to-line
      // tangency and a second point-on-circle equation.
      if (joint === null) {
        const line = ka === 'line' ? entityOf(sketch,c.a) : kb === 'line' ? entityOf(sketch,c.b) : undefined;
        const curve = ka === 'line' ? c.b : c.a;
        if (line?.kind === 'line') joint = [line.a,line.b].find(p => sketch.constraints.some(on => on.kind === 'on' && on.point === p && on.curve === curve)) ?? null;
      }
      if (joint !== null) {
        // Curves that meet end to end are tangent where they meet, as FreeCAD
        // makes them: their normals there at 0 or half a turn. Tangency to a
        // curve through a shared point would be counted twice.
        return [{ id, type: 'angle_via_point', crv1_id: E(c.a), crv2_id: E(c.b), p_id: P(joint), angle: tangentAngle(sketch, c.a, c.b, joint) }];
      }
      const [line, other, ko] = ka === 'line' ? [c.a, c.b, kb] : kb === 'line' ? [c.b, c.a, ka] : [null, null, null];
      if (line !== null && other !== null) {
        if (ko === 'circle') return [{ id, type: 'tangent_lc', l_id: E(line), c_id: E(other) }];
        if (ko === 'arc') return [{ id, type: 'tangent_la', l_id: E(line), a_id: E(other) }];
        return [];
      }
      if (ka === 'circle' && kb === 'circle') return [{ id, type: 'tangent_cc', c1_id: E(c.a), c2_id: E(c.b) }];
      if (ka === 'arc' && kb === 'arc') return [{ id, type: 'tangent_aa', a1_id: E(c.a), a2_id: E(c.b) }];
      if (ka === 'circle' && kb === 'arc') return [{ id, type: 'tangent_ca', c_id: E(c.a), a_id: E(c.b) }];
      if (ka === 'arc' && kb === 'circle') return [{ id, type: 'tangent_ca', c_id: E(c.b), a_id: E(c.a) }];
      return [];
    }
    case 'concentric': {
      const a = entityOf(sketch, c.a), b = entityOf(sketch, c.b);
      if (!a || !b || !('c' in a) || !('c' in b) || a.c === b.c) return [];
      return [{ id, type: 'p2p_coincident', p1_id: P(a.c), p2_id: P(b.c) }];
    }
    case 'midpoint': {
      const l = entityOf(sketch, c.line);
      if (l?.kind !== 'line') return [];
      return [{ id, type: 'p2p_symmetric_ppp', p1_id: P(l.a), p2_id: P(l.b), p_id: P(c.point) }];
    }
    case 'on': {
      const k = kind(c.curve);
      if (k === 'line') return [{ id, type: 'point_on_line_pl', p_id: P(c.point), l_id: E(c.curve) }];
      if (k === 'circle') return [{ id, type: 'point_on_circle', p_id: P(c.point), c_id: E(c.curve) }];
      if (k === 'arc') return [{ id, type: 'point_on_arc', p_id: P(c.point), a_id: E(c.curve) }];
      if ((k === 'ellipse' || k === 'ellipse-arc') && solvable(sketch, c.curve)) return [{ id, type: 'point_on_ellipse', p_id: P(c.point), e_id: E(c.curve) }];
      return [];
    }
    case 'symmetric': return [{ id, type: 'p2p_symmetric_ppl', p1_id: P(c.a), p2_id: P(c.b), l_id: E(c.line) }];
    case 'fix': return [];
    case 'distance': {
      if (c.axis === 'aligned') return [{ id, type: 'p2p_distance', p1_id: P(c.a), p2_id: P(c.b), distance: c.value }];
      const prop = c.axis === 'horizontal' ? 'x' : 'y';
      const a = point(sketch, c.a), b = point(sketch, c.b);
      const sign = (prop === 'x' ? b.x - a.x : b.y - a.y) < 0 ? -1 : 1;
      return [{ id, type: 'difference', param1: { o_id: P(c.a), prop }, param2: { o_id: P(c.b), prop }, difference: sign * c.value }];
    }
    case 'offset': return [{ id, type: 'p2l_distance', p_id: P(c.point), l_id: E(c.line), distance: c.value }];
    case 'diameter': case 'radius': {
      const k = kind(c.curve);
      const r = c.kind === 'diameter' ? c.value / 2 : c.value;
      if (k === 'circle') return [{ id, type: 'circle_radius', c_id: E(c.curve), radius: r }];
      if (k === 'arc') return [{ id, type: 'arc_radius', a_id: E(c.curve), radius: r }];
      if ((k === 'ellipse' || k === 'ellipse-arc') && solvable(sketch, c.curve)) return [{ id, type: 'equal', param1: { o_id: E(c.curve), prop: 'radmin' }, param2: r }];
      return [];
    }
    case 'angle': {
      const sign = lineAngle(sketch, c.a, c.b) < 0 ? -1 : 1;
      return [{ id, type: 'l2l_angle_ll', l1_id: E(c.a), l2_id: E(c.b), angle: (sign * c.value * Math.PI) / 180 }];
    }
  }
}

/** The curves with a size of their own besides their points: circles' radii, ellipses' minor semi-axes. */
const sized = (e: Entity): e is Extract<Entity, { r: number }> => e.kind === 'circle' || e.kind === 'ellipse' || e.kind === 'ellipse-arc';

/** Whether an ellipse can go to the solver: its major semi-axis no shorter than its minor. */
function solvable(sketch: Sketch, id: Id): boolean {
  const e = entityOf(sketch, id);
  if (e?.kind !== 'ellipse' && e?.kind !== 'ellipse-arc') return false;
  const { u } = ellipseFrame(sketch, e);
  return Math.hypot(u.x, u.y) > e.r + 1e-9 && e.r > 1e-9;
}

/** An ellipse as PlaneGCS keeps one: centre, a focus and the minor
 *  semi-axis, with the end of the major semi-axis held to its vertex. The
 *  focus is the solver's own point; the sketch keeps the vertex. */
function ellipsePrimitives(sketch: Sketch, e: Ellipse | EllipseArc): SketchPrimitive[] {
  if (!solvable(sketch, e.id)) return [];
  const { c, u } = ellipseFrame(sketch, e);
  const a = Math.hypot(u.x, u.y), k = Math.sqrt(a * a - e.r * e.r) / a;
  const focus = `f${e.id}`;
  const out: SketchPrimitive[] = [{ id: focus, type: 'point', x: c.x + u.x * k, y: c.y + u.y * k, fixed: false }];
  if (e.kind === 'ellipse') out.push({ id: E(e.id), type: 'ellipse', c_id: P(e.c), focus1_id: focus, radmin: e.r });
  else {
    const [start, end] = arcParams(sketch, e);
    out.push({ id: E(e.id), type: 'arc_of_ellipse', c_id: P(e.c), focus1_id: focus, start_id: P(e.a), end_id: P(e.b), start_angle: start, end_angle: end, radmin: e.r });
    out.push({ id: `r${e.id}`, type: 'arc_of_ellipse_rules', a_id: E(e.id) });
  }
  out.push({ id: `v${e.id}x`, type: 'internal_alignment_point2ellipse', e_id: E(e.id), p1_id: P(e.m), alignmentType: 0 });
  out.push({ id: `v${e.id}y`, type: 'internal_alignment_point2ellipse', e_id: E(e.id), p1_id: P(e.m), alignmentType: 1 });
  return out;
}

/** The whole sketch as solver primitives: points, then curves, then constraints. */
function primitives(sketch: Sketch, pulls: readonly Pull[], radii: ReadonlyMap<Id, number>, hold: boolean | ReadonlySet<Id>): SketchPrimitive[] {
  const fixed = new Set(sketch.constraints.flatMap((c) => (c.kind === 'fix' ? [c.point] : [])));
  const out: SketchPrimitive[] = sketch.points.map((p) => ({ id: P(p.id), type: 'point', x: p.x, y: p.y, fixed: fixed.has(p.id) }));
  for (const e of sketch.entities) {
    if (e.kind === 'line') out.push({ id: E(e.id), type: 'line', p1_id: P(e.a), p2_id: P(e.b) });
    else if (e.kind === 'circle') out.push({ id: E(e.id), type: 'circle', c_id: P(e.c), radius: e.r });
    else if (e.kind === 'ellipse' || e.kind === 'ellipse-arc') out.push(...ellipsePrimitives(sketch, e));
    // A spline's poles are points like any other; its curve follows them.
    else if (e.kind === 'spline') continue;
    else {
      const c = point(sketch, e.c), a = point(sketch, e.a), b = point(sketch, e.b);
      const start = angleOf(c, a);
      out.push({
        id: E(e.id), type: 'arc', c_id: P(e.c), radius: dist(c, a),
        start_angle: start, end_angle: start + sweep(start, angleOf(c, b)), start_id: P(e.a), end_id: P(e.b),
      });
      out.push({ id: `r${e.id}`, type: 'arc_rules', a_id: E(e.id) });
    }
  }
  for (const c of sketch.constraints) out.push(...translate(sketch, c));
  pulls.forEach((pull, i) => {
    out.push({ id: `t${i}x`, type: 'coordinate_x', p_id: P(pull.point), x: pull.x, temporary: true });
    out.push({ id: `t${i}y`, type: 'coordinate_y', p_id: P(pull.point), y: pull.y, temporary: true });
  });
  if (hold) {
    // Every other point keeps lightly to where it is, so an edit moves only
    // what it must, as Fusion's sketch does, rather than the whole sketch.
    const pulled = new Set(pulls.map((p) => p.point));
    const loose = hold === true ? new Set<Id>() : hold;
    for (const p of sketch.points) {
      if (pulled.has(p.id) || loose.has(p.id)) continue;
      out.push({ id: `h${p.id}x`, type: 'coordinate_x', p_id: P(p.id), x: p.x, temporary: true, scale: 0.01 });
      out.push({ id: `h${p.id}y`, type: 'coordinate_y', p_id: P(p.id), y: p.y, temporary: true, scale: 0.01 });
    }
  }
  radii.forEach((r, entity) => {
    const e = entityOf(sketch, entity);
    if (e?.kind === 'circle') out.push({ id: `t${entity}r`, type: 'circle_radius', c_id: E(entity), radius: r, temporary: true });
    else if (e && (e.kind === 'ellipse' || e.kind === 'ellipse-arc') && solvable(sketch, entity)) out.push({ id: `t${entity}r`, type: 'equal', param1: { o_id: E(entity), prop: 'radmin' }, param2: r, temporary: true });
  });
  return out;
}

/** The sketch in groups that share no point, curve or constraint: points
 *  tied by curves and relations, with those curves and relations. Each
 *  group solves on its own, so a pattern of a hundred copies is a hundred
 *  small systems rather than one too large to solve. */
export function clusters(sketch: Sketch): Sketch[] {
  const parent = new Map<Id, Id>();
  const find = (x: Id): Id => {
    let root = x;
    while (parent.get(root) !== root) root = parent.get(root)!;
    while (x !== root) { const up = parent.get(x)!; parent.set(x, root); x = up; }
    return root;
  };
  const union = (a: Id, b: Id) => { const ra = find(a), rb = find(b); if (ra !== rb) parent.set(ra, rb); };
  for (const p of sketch.points) parent.set(p.id, p.id);
  const ends = new Map(sketch.entities.map((e) => [e.id, pointsOf(e).filter((id) => parent.has(id))]));
  const tied = (c: Constraint): Id[] => constraintRefs(c).flatMap((r) => ends.get(r) ?? (parent.has(r) ? [r] : []));
  const join = (ids: Id[]) => { for (let i = 1; i < ids.length; i++) union(ids[0]!, ids[i]!); };
  for (const ids of ends.values()) join(ids);
  for (const c of sketch.constraints) join(tied(c));
  const groups = new Map<Id, Sketch>();
  const group = (id: Id): Sketch => {
    const root = find(id);
    let g = groups.get(root);
    if (!g) groups.set(root, (g = { points: [], entities: [], constraints: [], next: sketch.next }));
    return g;
  };
  for (const p of sketch.points) group(p.id).points.push(p);
  for (const e of sketch.entities) { const first = ends.get(e.id)![0]; if (first !== undefined) group(first).entities.push(e); }
  for (const c of sketch.constraints) { const first = tied(c)[0]; if (first !== undefined) group(first).constraints.push(c); }
  return [...groups.values()];
}

/** Solved groups by content, so a group an edit did not touch is not solved again. */
const solvedGroups = new Map<string, Solved>();

/** Solves the sketch, with points pulled towards positions and circles towards radii. */
export function solve(sketch: Sketch, pulls: readonly Pull[] = [], radii: ReadonlyMap<Id, number> = new Map(), hold: boolean | ReadonlySet<Id> = false): Solved {
  if (!wrapper) return { ok: true, sketch, conflicting: [], redundant: [], dof: -1 };
  const groups = clusters(sketch);
  if (groups.length <= 1) return solveGroup(sketch, pulls, radii, hold);
  const points = new Map<Id, { x: number; y: number }>(), sizes = new Map<Id, number>();
  const conflicting: Id[] = [], redundant: Id[] = [];
  let dof = 0, ok = true;
  for (const group of groups) {
    const own = new Set(group.points.map((p) => p.id));
    const curves = new Set(group.entities.map((e) => e.id));
    const pulled = pulls.filter((p) => own.has(p.point));
    const sized = new Map([...radii].filter(([e]) => curves.has(e)));
    const loose = typeof hold === 'boolean' ? hold : group.points.filter((p) => hold.has(p.id)).map((p) => p.id);
    const key = JSON.stringify([group.points, group.entities, group.constraints, pulled, [...sized], loose]);
    let result = solvedGroups.get(key);
    if (!result) {
      result = solveGroup(group, pulled, sized, hold);
      if (solvedGroups.size > 4000) solvedGroups.clear();
      if (wrapper) solvedGroups.set(key, result);
    }
    ok &&= result.ok;
    conflicting.push(...result.conflicting);
    redundant.push(...result.redundant);
    dof += result.dof;
    for (const p of result.sketch.points) points.set(p.id, p);
    for (const e of result.sketch.entities) if (e.kind === 'circle' || e.kind === 'ellipse' || e.kind === 'ellipse-arc') sizes.set(e.id, e.r);
  }
  if (!ok) return { ok, sketch, conflicting, redundant, dof };
  const solved = clone(sketch);
  for (const p of solved.points) { const at = points.get(p.id); if (at) { p.x = at.x; p.y = at.y; } }
  for (const e of solved.entities) if (e.kind === 'circle' || e.kind === 'ellipse' || e.kind === 'ellipse-arc') e.r = sizes.get(e.id) ?? e.r;
  return { ok, sketch: solved, conflicting, redundant, dof };
}

/** Solves one group of the sketch as one system. */
function solveGroup(sketch: Sketch, pulls: readonly Pull[], radii: ReadonlyMap<Id, number>, hold: boolean | ReadonlySet<Id>): Solved {
  try {
    return solveSystem(sketch, pulls, radii, hold);
  } catch {
    // Out of memory or aborted inside the WebAssembly: that solver is gone.
    restart();
    return { ok: false, sketch, conflicting: [], redundant: [], dof: -1 };
  }
}

function solveSystem(sketch: Sketch, pulls: readonly Pull[], radii: ReadonlyMap<Id, number>, hold: boolean | ReadonlySet<Id>): Solved {
  if (!wrapper) return { ok: true, sketch, conflicting: [], redundant: [], dof: -1 };
  const w = wrapper;
  w.clear_data();
  w.push_primitives_and_params(primitives(sketch, pulls, radii, hold));
  const status = w.solve();
  const conflicting = [...new Set(w.get_gcs_conflicting_constraints().map(owner).filter((id): id is Id => id !== null))];
  const redundant = [...new Set(w.get_gcs_redundant_constraints().map(owner).filter((id): id is Id => id !== null))];
  const dof = w.gcs.dof();
  const ok = (status === SolveStatus.Success || status === SolveStatus.Converged) && !conflicting.length;
  if (!ok) return { ok, sketch, conflicting, redundant, dof };
  w.apply_solution();
  const solved = clone(sketch);
  const byId = new Map(solved.points.map((p) => [P(p.id), p]));
  const curves = new Map(solved.entities.map((e) => [E(e.id), e]));
  for (const prim of w.sketch_index.get_primitives()) {
    if (prim.type === 'point') {
      const p = byId.get(String(prim.id));
      if (p && Number.isFinite(prim.x) && Number.isFinite(prim.y)) { p.x = prim.x; p.y = prim.y; }
    } else if (prim.type === 'circle') {
      const e = curves.get(String(prim.id));
      if (e?.kind === 'circle' && Number.isFinite(prim.radius) && prim.radius > 0) e.r = prim.radius;
    } else if (prim.type === 'ellipse' || prim.type === 'arc_of_ellipse') {
      const e = curves.get(String(prim.id));
      if ((e?.kind === 'ellipse' || e?.kind === 'ellipse-arc') && Number.isFinite(prim.radmin) && prim.radmin > 0) e.r = prim.radmin;
    }
  }
  return { ok, sketch: solved, conflicting, redundant, dof };
}

/** Which points and circles can still move: the sketch's blue geometry.
 *  Each group is nudged point by point and watched; copies alike up to
 *  where they lie share one answer. Null without a solver. */
export function freedom(sketch: Sketch, limit = 300): { points: Set<Id>; circles: Set<Id> } | null {
  if (!wrapper) return null;
  const free = { points: new Set<Id>(), circles: new Set<Id>() };
  const known = new Map<string, { points: number[]; circles: number[] }>();
  for (const group of clusters(sketch)) {
    const base = solve(group);
    if (base.ok && base.dof === 0) continue;
    const solved = base.ok ? base.sketch : group;
    const shape = likeness(solved);
    let found = known.get(shape);
    if (!found) {
      found = base.ok && solved.points.length <= limit ? nudged(solved) : everything(solved);
      known.set(shape, found);
    }
    for (const i of found.points) free.points.add(solved.points[i]!.id);
    for (const i of found.circles) free.circles.add(solved.entities[i]!.id);
  }
  return free;
}

/** Every point and circle of a group, by place in its lists. */
function everything(group: Sketch): { points: number[]; circles: number[] } {
  return {
    points: group.points.map((_, i) => i),
    circles: group.entities.flatMap((e, i) => (sized(e) ? [i] : [])),
  };
}

/** What moves in one solved group when each point and circle is nudged. */
function nudged(group: Sketch): { points: number[]; circles: number[] } {
  const step = 0.5;
  const found = { points: [] as number[], circles: [] as number[] };
  group.points.forEach((p, i) => {
    const moved = solveGroup(group, [{ point: p.id, x: p.x + step, y: p.y + step }], new Map(), false);
    if (!moved.ok || dist(p, point(moved.sketch, p.id)) > step * 0.05) found.points.push(i);
  });
  group.entities.forEach((e, i) => {
    if (!sized(e)) return;
    const grown = solveGroup(group, [], new Map([[e.id, e.r + step]]), false);
    const r = grown.sketch.entities.find((x) => x.id === e.id);
    if (!grown.ok || (r && sized(r) && Math.abs(r.r - e.r) > step * 0.05)) found.circles.push(i);
  });
  // Freedom left that no nudge found is still freedom: none of it is defined.
  return found.points.length || found.circles.length ? found : everything(group);
}

/** A group's make-up without its ids and position: copies of one shape
 *  moved elsewhere read alike, so they are nudged once. */
function likeness(group: Sketch): string {
  const index = new Map<Id, string>();
  group.points.forEach((p, i) => index.set(p.id, `p${i}`));
  group.entities.forEach((e, i) => index.set(e.id, `e${i}`));
  const o = group.points[0] ?? { x: 0, y: 0 };
  const round = (v: number) => Math.round(v * 1e6) / 1e6;
  const named = (key: string, value: unknown): unknown =>
    typeof value === 'number' && key !== 'value' && key !== 'r' && index.has(value) ? index.get(value) : value;
  const plain = (o: object, skip: string[]) => Object.entries(o).filter(([k]) => !skip.includes(k)).map(([k, v]) => [k, named(k, v)]);
  return JSON.stringify([
    group.points.map((p) => [round(p.x - o.x), round(p.y - o.y)]),
    group.entities.map((e) => plain(e, ['id', 'construction'])),
    group.constraints.map((c) => plain(c, ['id', 'at'])),
  ]);
}

/** A curve is fully defined when none of its points, nor a circle's size, can move. */
export function defined(entity: Entity, free: { points: Set<Id>; circles: Set<Id> }): boolean {
  const pts = pointsOf(entity);
  if (pts.some((p) => free.points.has(p))) return false;
  return !sized(entity) || !free.circles.has(entity.id);
}

/** The points an edit is about: those of the constraints it adds or changes,
 *  and of the curves those refer to, with every point of new curves. They
 *  move freely while the rest keeps to where it was. */
export function looseAfter(previous: Sketch, next: Sketch): Set<Id> {
  const before = new Map(previous.constraints.map((c) => [c.id, JSON.stringify(c)]));
  const loose = new Set<Id>();
  const known = new Set(previous.entities.map((e) => e.id));
  const points = new Set(next.points.map((p) => p.id));
  const curves = new Map(next.entities.map((e) => [e.id, e]));
  const addCurve = (id: Id) => { const e = curves.get(id); if (e) for (const p of pointsOf(e)) loose.add(p); };
  for (const c of next.constraints) {
    if (before.get(c.id) === JSON.stringify(c)) continue;
    for (const ref of constraintRefs(c)) { if (points.has(ref)) loose.add(ref); else addCurve(ref); }
  }
  for (const e of next.entities) if (!known.has(e.id)) addCurve(e.id);
  return loose;
}

/** How each pair of lines meeting at a point turns there, by the sign of
 *  their cross product; nearly straight pairs are left out. */
function turnSigns(s: Sketch): Map<string, number> {
  const lines = s.entities.filter((e): e is Extract<Entity, { kind: 'line' }> => e.kind === 'line' && !e.construction);
  const at = new Map<Id, Extract<Entity, { kind: 'line' }>[]>();
  for (const l of lines) for (const p of [l.a, l.b]) at.set(p, [...(at.get(p) ?? []), l]);
  const out = new Map<string, number>();
  for (const [p, meeting] of at) for (let i = 0; i < meeting.length; i++) for (let j = i + 1; j < meeting.length; j++) {
    const a = meeting[i]!, b = meeting[j]!;
    const from = point(s, p), ta = point(s, a.a === p ? a.b : a.a), tb = point(s, b.a === p ? b.b : b.a);
    const u = { x: ta.x - from.x, y: ta.y - from.y }, v = { x: tb.x - from.x, y: tb.y - from.y };
    const len = Math.hypot(u.x, u.y) * Math.hypot(v.x, v.y);
    const cross = u.x * v.y - u.y * v.x;
    if (len > 1e-9 && Math.abs(cross) / len > 0.02) out.set(`${p}:${a.id}/${b.id}`, Math.sign(cross));
  }
  return out;
}

/** Whether a corner that was there before turns the other way now: part
 *  of the shape folded over or mirrored. */
function folded(before: Sketch, after: Sketch): boolean {
  const a = turnSigns(before), b = turnSigns(after);
  for (const [key, sign] of a) if (b.has(key) && b.get(key) !== sign) return true;
  return false;
}

/** The dimensions whose value alone changed, old and new. */
function valueChanges(previous: Sketch, next: Sketch): Map<Id, [number, number]> {
  const out = new Map<Id, [number, number]>();
  if (previous.points.length !== next.points.length || previous.entities.length !== next.entities.length) return out;
  const before = new Map(previous.constraints.map((c) => [c.id, c]));
  for (const c of next.constraints) {
    const old = before.get(c.id);
    if (!old || !('value' in c) || !('value' in old) || old.kind !== c.kind || old.value === c.value) continue;
    if (JSON.stringify({ ...old, value: 0 }) !== JSON.stringify({ ...c, value: 0 })) continue;
    out.set(c.id, [old.value, c.value]);
  }
  return out;
}

/** Dimension edits prefer the older reference geometry without adding a
 *  permanent fix. A large change that would fold part of the shape over,
 *  as the solver may jump to a mirrored solution, is made again in small
 *  steps from one solution to the next, so the shape follows the value. */
export function solveEdit(previous: Sketch, next: Sketch): Solved {
  const direct = solveEditDirect(previous, next);
  if (direct.ok && !folded(previous, direct.sketch)) return direct;
  const changes = valueChanges(previous, next);
  if (!changes.size) return direct;
  let steps = 1;
  for (const [id, [from, to]] of changes) {
    const kind = next.constraints.find((c) => c.id === id)?.kind;
    const n = kind === 'angle' ? Math.abs(to - from) / 5 : from > 0 && to > 0 ? Math.abs(Math.log(to / from)) / Math.log(1.1) : 1;
    steps = Math.max(steps, Math.ceil(n));
  }
  steps = Math.min(60, Math.max(2, steps));
  let current = previous;
  let result: Solved = direct;
  for (let k = 1; k <= steps; k++) {
    const stage = clone(next);
    stage.points = clone(current).points;
    for (const c of stage.constraints) {
      const change = changes.get(c.id);
      if (!change || !('value' in c)) continue;
      const [from, to] = change;
      const kind = c.kind;
      (c as { value: number }).value = k === steps ? to
        : kind !== 'angle' && from > 0 && to > 0 ? from * Math.pow(to / from, k / steps)
        : from + ((to - from) * k) / steps;
    }
    result = solveEditDirect(current, stage);
    if (!result.ok) return direct;
    current = result.sketch;
  }
  return folded(previous, result.sketch) && direct.ok ? direct : result;
}

function solveEditDirect(previous: Sketch, next: Sketch): Solved {
  const fillets = resizedFillets(previous, next);
  if (fillets.error) return {ok:false,sketch:next,conflicting:[],redundant:[],dof:-1,reason:fillets.error};
  next = fillets.sketch;
  const before = new Map(previous.constraints.map(c => [c.id, JSON.stringify(c)]));
  const anchors = new Set<Id>();
  const previousGroups = clusters(previous);
  const oldGroup = new Map(previousGroups.flatMap((g, i) => g.points.map(p => [p.id, i] as const)));
  const preserve = new Set<number>();
  const known = new Set(previous.points.map(p => p.id));
  for (const c of next.constraints) {
    if (before.get(c.id) === JSON.stringify(c) || ('driven' in c && c.driven)) continue;
    if (c.kind === 'distance') {
      const earlier = Math.min(c.a, c.b), later = Math.max(c.a, c.b);
      // Between separate objects preserve the earlier curve, not just its endpoint.
      const curve = next.entities.find(e => pointsOf(e).includes(earlier));
      const own = curve ? pointsOf(curve) : [earlier];
      for (const id of own.includes(later) ? [earlier] : own) if (known.has(id)) anchors.add(id);
    } else if (c.kind === 'offset') {
      const line = entityOf(next, c.line);
      const owner = next.entities.find(e => pointsOf(e).includes(c.point));
      const reference = line && owner ? (line.id < owner.id ? line : owner) : line;
      for (const id of reference ? pointsOf(reference) : [c.point]) if (known.has(id)) anchors.add(id);
    } else if (c.kind === 'angle') {
      const reference = entityOf(previous, Math.min(c.a, c.b));
      const rotated = entityOf(previous, Math.max(c.a, c.b));
      if (reference && rotated) {
        const group = previousGroups[oldGroup.get(pointsOf(reference)[0]!)!];
        const otherPoints = new Set(previous.entities.filter(e => e.id !== rotated.id).flatMap(pointsOf));
        for (const p of group?.points ?? []) if (otherPoints.has(p.id)) anchors.add(p.id);
      }
    } else if (c.kind === 'radius' || c.kind === 'diameter') {
      const curve = entityOf(next, c.curve);
      if (curve?.kind === 'circle' && known.has(curve.c)) anchors.add(curve.c);
    }
  }
    // An inter-object dimension should move the newer object as a whole, rather
  // than resize the older outline around its anchored edge.
  for (const c of next.constraints) {
    if (before.get(c.id) === JSON.stringify(c) || ('driven' in c && c.driven)) continue;
    if (!['distance', 'offset', 'angle'].includes(c.kind)) continue;
    const groups = new Set(constraintRefs(c).flatMap(ref => {
      const e = entityOf(previous, ref);
      return (e ? pointsOf(e) : [ref]).flatMap(id => oldGroup.has(id) ? [oldGroup.get(id)!] : []);
    }));
    if (groups.size > 1) for (const id of anchors) {
      const group = oldGroup.get(id); if (group !== undefined && groups.has(group)) preserve.add(group);
    }
  }
  for (const group of preserve) for (const p of previousGroups[group]!.points) anchors.add(p.id);
  for (const group of previousGroups) if (group.entities.some(e => fillets.curves.has(e.id))) {
    for (const p of group.points) if (!fillets.moving.has(p.id)) anchors.add(p.id);
  }
  const loose = looseAfter(previous, next);
  if (!anchors.size) return solve(next, [], new Map(), loose);
  // No competing holds in the affected component: the reference stays put while
  // its connected geometry changes size. Explicit sketch constraints still win.
  for (const group of clusters(next)) if (group.points.some(p => anchors.has(p.id))) {
    for (const p of group.points) loose.add(p.id);
  }
  const pulls = previous.points.filter(p => anchors.has(p.id)).map(p => ({ point: p.id, x: p.x, y: p.y }));
  const referencePulls = [...pulls];
  let angleChanged = false;
  for (const c of next.constraints) {
    if (c.kind !== 'angle' || c.driven || before.get(c.id) === JSON.stringify(c)) continue;
    angleChanged = true;
    const rotated = entityOf(previous, Math.max(c.a,c.b));
    const reference = entityOf(previous, Math.min(c.a,c.b));
    if (rotated?.kind !== 'line' || reference?.kind !== 'line') continue;
    const a=point(previous,rotated.a), b=point(previous,rotated.b);
    const ra=point(previous,reference.a), rb=point(previous,reference.b);
    const sign=lineAngle(previous,c.a,c.b)<0?-1:1;
    const delta=(c.a===reference.id?sign:-sign)*c.value*Math.PI/180;
    const angle=angleOf(ra,rb)+delta, length=dist(a,b);
    if (anchors.has(rotated.a) && !anchors.has(rotated.b))
      pulls.push({point:rotated.b,x:a.x+length*Math.cos(angle),y:a.y+length*Math.sin(angle)});
    else if (anchors.has(rotated.b) && !anchors.has(rotated.a))
      pulls.push({point:rotated.a,x:b.x-length*Math.cos(angle),y:b.y-length*Math.sin(angle)});
    else if (!anchors.has(rotated.a) && !anchors.has(rotated.b)) {
      pulls.push({point:rotated.a,x:a.x,y:a.y});
      pulls.push({point:rotated.b,x:a.x+length*Math.cos(angle),y:a.y+length*Math.sin(angle)});
    }
  }
  let anchored = solve(next, pulls, new Map(), loose);
  const referenceMoved = (result: Solved) => result.ok && previous.points.some(p => anchors.has(p.id) && dist(p,point(result.sketch,p.id))>1e-5);
  if (angleChanged && (!anchored.ok || referenceMoved(anchored))) {
    // A point constrained to an edge can slide; keeping the line's old length
    // is secondary to keeping its reference contour unchanged.
    anchored = solve(next, referencePulls, new Map(), loose);
    if (!anchored.ok || referenceMoved(anchored)) return {ok:false,sketch:next,conflicting:anchored.conflicting,redundant:[],dof:-1,reason:'This angle cannot change with the existing joints and relations without distorting the reference contour.'};
  }
  if (fillets.curves.size) {
    if (!anchored.ok || previous.points.some(p => anchors.has(p.id) && dist(p,point(anchored.sketch,p.id))>1e-5))
      return {ok:false,sketch:next,conflicting:anchored.conflicting,redundant:[],dof:-1,reason:'The radius change contradicts the existing contour or its relations. The sketch stays as it was.'};
  }
  return anchored.ok ? anchored : solve(next, [], new Map(), looseAfter(previous, next));
}
