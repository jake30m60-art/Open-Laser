// The sketch's constraints, solved by FreeCAD's PlaneGCS (WebAssembly).
// Points become solver points, curves refer to them, and every relation
// and dimension becomes one or more solver constraints. Dragging adds
// temporary constraints that pull points towards the cursor as far as the
// real constraints allow, as FreeCAD's and Fusion's sketchers do.
import { make_gcs_wrapper, SolveStatus, type GcsWrapper, type SketchPrimitive } from '@salusoft89/planegcs';
import { angleOf, dist, sweep } from './geometry';
import { clone, point, type Constraint, type Entity, type Id, type Sketch } from './model';

let wrapper: GcsWrapper | null = null;
let loading: Promise<void> | null = null;

/** Loads the solver once; `wasm` locates its WebAssembly in a browser build. */
export function initSolver(wasm?: string): Promise<void> {
  loading ??= make_gcs_wrapper(wasm).then((w) => { wrapper = w; });
  return loading;
}

export const solverReady = (): boolean => wrapper !== null;

/** A point held towards a position while the rest settles. */
export interface Pull { point: Id; x: number; y: number }

export interface Solved {
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
  const ends = (id: Id): Id[] => { const e = entityOf(sketch, id); return !e || e.kind === 'circle' ? [] : [e.a, e.b]; };
  const theirs = ends(b);
  return ends(a).find((p) => theirs.includes(p)) ?? null;
}

/** A curve's normal at a point in FreeCAD's convention: a line's direction
 *  turned a quarter counter-clockwise, a circle's towards its centre. */
function normalAt(sketch: Sketch, id: Id, at: Id): { x: number; y: number } {
  const e = entityOf(sketch, id)!;
  const p = point(sketch, at);
  if (e.kind === 'line') { const a = point(sketch, e.a), b = point(sketch, e.b); return { x: -(b.y - a.y), y: b.x - a.x }; }
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
      const joint = sharedEnd(sketch, c.a, c.b);
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
      if (!a || !b || a.kind === 'line' || b.kind === 'line' || a.c === b.c) return [];
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
      return [];
    }
    case 'angle': {
      const sign = lineAngle(sketch, c.a, c.b) < 0 ? -1 : 1;
      return [{ id, type: 'l2l_angle_ll', l1_id: E(c.a), l2_id: E(c.b), angle: (sign * c.value * Math.PI) / 180 }];
    }
  }
}

/** The whole sketch as solver primitives: points, then curves, then constraints. */
function primitives(sketch: Sketch, pulls: readonly Pull[], radii: ReadonlyMap<Id, number>): SketchPrimitive[] {
  const fixed = new Set(sketch.constraints.flatMap((c) => (c.kind === 'fix' ? [c.point] : [])));
  const out: SketchPrimitive[] = sketch.points.map((p) => ({ id: P(p.id), type: 'point', x: p.x, y: p.y, fixed: fixed.has(p.id) }));
  for (const e of sketch.entities) {
    if (e.kind === 'line') out.push({ id: E(e.id), type: 'line', p1_id: P(e.a), p2_id: P(e.b) });
    else if (e.kind === 'circle') out.push({ id: E(e.id), type: 'circle', c_id: P(e.c), radius: e.r });
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
  radii.forEach((r, entity) => {
    const e = entityOf(sketch, entity);
    if (e?.kind === 'circle') out.push({ id: `t${entity}r`, type: 'circle_radius', c_id: E(entity), radius: r, temporary: true });
  });
  return out;
}

/** Solves the sketch, with points pulled towards positions and circles towards radii. */
export function solve(sketch: Sketch, pulls: readonly Pull[] = [], radii: ReadonlyMap<Id, number> = new Map()): Solved {
  if (!wrapper) return { ok: true, sketch, conflicting: [], redundant: [], dof: -1 };
  const w = wrapper;
  w.clear_data();
  w.push_primitives_and_params(primitives(sketch, pulls, radii));
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
    }
  }
  return { ok, sketch: solved, conflicting, redundant, dof };
}

/** Which points and circles can still move: the sketch's blue geometry.
 *  Each is nudged in turn and watched; null when the sketch is too large
 *  to check quickly. */
export function freedom(sketch: Sketch, limit = 300): { points: Set<Id>; circles: Set<Id> } | null {
  if (!wrapper) return null;
  const free = { points: new Set<Id>(), circles: new Set<Id>() };
  const base = solve(sketch);
  if (!base.ok) return null;
  if (base.dof === 0) return free;
  if (sketch.points.length > limit) return null;
  const step = 0.5;
  for (const p of base.sketch.points) {
    const moved = solve(base.sketch, [{ point: p.id, x: p.x + step, y: p.y + step }]);
    if (!moved.ok || dist(p, point(moved.sketch, p.id)) > step * 0.05) free.points.add(p.id);
  }
  for (const e of base.sketch.entities) {
    if (e.kind !== 'circle') continue;
    const grown = solve(base.sketch, [], new Map([[e.id, e.r + step]]));
    const r = grown.sketch.entities.find((x) => x.id === e.id);
    if (!grown.ok || (r?.kind === 'circle' && Math.abs(r.r - e.r) > step * 0.05)) free.circles.add(e.id);
  }
  // Freedom left that no nudge found is still freedom: none of it is defined.
  if (!free.points.size && !free.circles.size) {
    for (const p of base.sketch.points) free.points.add(p.id);
  }
  return free;
}

/** A curve is fully defined when none of its points, nor a circle's size, can move. */
export function defined(entity: Entity, free: { points: Set<Id>; circles: Set<Id> }): boolean {
  const pts = entity.kind === 'line' ? [entity.a, entity.b] : entity.kind === 'circle' ? [entity.c] : [entity.c, entity.a, entity.b];
  if (pts.some((p) => free.points.has(p))) return false;
  return entity.kind !== 'circle' || !free.circles.has(entity.id);
}
