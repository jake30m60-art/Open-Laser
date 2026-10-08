import { t } from '../i18n/index';
// The tools that act on what is drawn: Fusion's modify tools, its
// constraints and its sketch dimension. Each takes clicks on points and
// curves, may offer heads-up values, previews its result, and returns the
// sketch to commit; the page solves it before keeping it.
import { dist, distanceTo, shapeOf, sketchBounds, type Shape, type Vec } from './geometry';
import { addConstraint, clone, merge, point, pointsOf, type Dimension, type Entity, type Id, type NewConstraint, type Sketch } from './model';
import { breakAt, chains, chamfer, circularPattern, extend, fillet, mirror, offset, rectangularPattern, stretch, trim, type Outcome } from './modify';
import { axisFor, labelAnchor, measure } from './annotations';
import type { Field, Locked } from './tools';

export type ConstraintId = 'coincident' | 'collinear' | 'concentric' | 'midpoint' | 'fix' | 'parallel' | 'perpendicular' | 'hv' | 'tangent' | 'equal' | 'symmetric';
export type ModifyId = 'trim' | 'extend' | 'break' | 'fillet' | 'chamfer' | 'offset' | 'mirror' | 'rpattern' | 'cpattern';
export type ActionId = ModifyId | ConstraintId | 'dimension';

/** What lies under a click: a point first, then a curve, and where. */
export interface Hit { point: Id | null; entity: Id | null; at: Vec }

export interface Context { sketch: Sketch; selection: ReadonlySet<Id> }

export type Step =
  | { kind: 'more' }
  /** Replace the selection, as picking an offset's chain does. */
  | { kind: 'select'; ids: Id[] }
  /** Add a curve to the selection or take it out. */
  | { kind: 'toggle'; id: Id }
  /** Keep this sketch; `done` ends the action, `edit` asks a new dimension's value. */
  | { kind: 'apply'; sketch: Sketch; select?: Id[]; done?: boolean; edit?: Id }
  | { kind: 'error'; message: string };

export interface Preview { shapes: Shape[]; removed: Shape[]; dimension: Dimension | null }

const none = (): Preview => ({ shapes: [], removed: [], dimension: null });
const entityOf = (s: Sketch, id: Id | null): Entity | undefined => (id === null ? undefined : s.entities.find((e) => e.id === id));
const outcome = (o: Outcome, done = false): Step => ('error' in o ? { kind: 'error', message: o.error } : { kind: 'apply', sketch: o.sketch, select: o.created, done });
const createdShapes = (o: Outcome): Shape[] => ('error' in o ? [] : o.created.map((id) => shapeOf(o.sketch, o.sketch.entities.find((e) => e.id === id)!)));

export abstract class Action {
  abstract readonly id: ActionId;
  abstract readonly name: string;
  protected picks: Hit[] = [];
  get busy(): boolean { return this.picks.length > 0; }
  reset(): void { this.picks = []; }
  /** What has been picked so far, to show it. */
  picked(): { points: Id[]; curves: Id[] } {
    return { points: this.picks.flatMap((h) => (h.point !== null ? [h.point] : [])), curves: this.picks.flatMap((h) => (h.point === null && h.entity !== null ? [h.entity] : [])) };
  }
  abstract hint(ctx: Context): string;
  fields(_ctx: Context, _cursor: Vec): Field[] { return []; }
  preview(_ctx: Context, _cursor: Vec, _hover: Hit, _locked: Locked): Preview { return none(); }
  abstract click(ctx: Context, hit: Hit, locked: Locked): Step;
  /** Enter: apply with what is set, where the tool allows it. */
  enter(_ctx: Context, _cursor: Vec, _locked: Locked): Step { return { kind: 'more' }; }
}

// ---- Trim, extend, break: one click on a curve -----------------------------------------------------

class CurveClick extends Action {
  constructor(readonly id: 'trim' | 'extend' | 'break', readonly name: string, private readonly run: (s: Sketch, id: Id, at: Vec) => Outcome) { super(); }
  hint(): string {
    return this.id === 'trim' ? t("Click the stretch of a curve to remove, up to where others cross it")
      : this.id === 'extend' ? t("Click near the end of a line or arc to lengthen it to the next curve")
        : t("Click a curve to split it where others cross it");
  }
  override preview(ctx: Context, _cursor: Vec, hover: Hit): Preview {
    if (hover.entity === null) return none();
    if (this.id === 'trim') { const cut = stretch(ctx.sketch, hover.entity, hover.at); return { ...none(), removed: cut ? [cut] : [] }; }
    const o = this.run(ctx.sketch, hover.entity, hover.at);
    return { ...none(), shapes: createdShapes(o) };
  }
  click(ctx: Context, hit: Hit): Step {
    if (hit.entity === null) return { kind: 'more' };
    return outcome(this.run(ctx.sketch, hit.entity, hit.at));
  }
}

// ---- Fillet and chamfer: a corner, then a size --------------------------------------------------------

class Corner extends Action {
  private size: number;
  constructor(readonly id: 'fillet' | 'chamfer', readonly name: string) { super(); this.size = id === 'fillet' ? 5 : 2; }
  hint(): string { return this.picks.length ? t("Type the {0} · Enter or click to apply", this.id === 'fillet' ? 'radius' : 'distance') : t("Click a corner where two lines meet"); }
  private key(): string { return this.id === 'fillet' ? 'radius' : 'distance'; }
  override fields(): Field[] {
    return this.picks.length ? [{ key: this.key(), label: this.id === 'fillet' ? t("Radius") : t("Distance"), kind: 'length', value: this.size }] : [];
  }
  private run(s: Sketch, locked: Locked): Outcome {
    const size = locked[this.key()] ?? this.size;
    const v = this.picks[0]!.point!;
    return this.id === 'fillet' ? fillet(s, v, size) : chamfer(s, v, size);
  }
  override preview(ctx: Context, _c: Vec, _h: Hit, locked: Locked): Preview {
    return this.picks.length ? { ...none(), shapes: createdShapes(this.run(ctx.sketch, locked)) } : none();
  }
  click(ctx: Context, hit: Hit, locked: Locked): Step {
    if (!this.picks.length) {
      if (hit.point === null) return { kind: 'error', message: t("Click the corner point where two lines meet.") };
      this.picks.push(hit);
      return { kind: 'more' };
    }
    return this.enter(ctx, hit.at, locked);
  }
  override enter(ctx: Context, _c: Vec, locked: Locked): Step {
    if (!this.picks.length) return { kind: 'more' };
    this.size = locked[this.key()] ?? this.size;
    const step = outcome(this.run(ctx.sketch, locked));
    this.reset();
    return step;
  }
}

// ---- Offset: a chain, then a side and distance -------------------------------------------------------

class Offset extends Action {
  readonly id = 'offset';
  readonly name = t("Offset");
  hint(ctx: Context): string { return ctx.selection.size ? t("Move to the side, type the distance · Enter or click to apply") : t("Click a curve: its connected profile is offset"); }
  private distance(ctx: Context, cursor: Vec, locked: Locked): number {
    if (locked.distance !== undefined) return locked.distance;
    let best = Infinity;
    for (const id of ctx.selection) {
      const e = entityOf(ctx.sketch, id);
      if (!e) continue;
      const shape = shapeOf(ctx.sketch, e);
      const d = shape.kind === 'line' ? lineDistance(shape.a, shape.b, cursor) : shape.kind === 'poly' ? distanceTo(shape, cursor) : Math.abs(dist(cursor, shape.c) - shape.r);
      best = Math.min(best, d);
    }
    return Number.isFinite(best) ? Math.round(best * 100) / 100 : 1;
  }
  override fields(ctx: Context, cursor: Vec): Field[] {
    return ctx.selection.size ? [{ key: 'distance', label: t("Distance"), kind: 'length', value: this.distance(ctx, cursor, {}) }] : [];
  }
  override preview(ctx: Context, cursor: Vec, _h: Hit, locked: Locked): Preview {
    if (!ctx.selection.size) return none();
    return { ...none(), shapes: createdShapes(offset(ctx.sketch, [...ctx.selection], this.distance(ctx, cursor, locked), cursor)) };
  }
  click(ctx: Context, hit: Hit, locked: Locked): Step {
    if (!ctx.selection.size) {
      if (hit.entity === null) return { kind: 'more' };
      const chain = chains(ctx.sketch, [...ctx.sketch.entities.filter((e) => !e.construction).map((e) => e.id)])
        .find((c) => c.links.some((l) => l.id === hit.entity));
      return { kind: 'select', ids: chain ? chain.links.map((l) => l.id) : [hit.entity] };
    }
    return this.enter(ctx, hit.at, locked);
  }
  override enter(ctx: Context, cursor: Vec, locked: Locked): Step {
    if (!ctx.selection.size) return { kind: 'more' };
    return outcome(offset(ctx.sketch, [...ctx.selection], this.distance(ctx, cursor, locked), cursor), true);
  }
}

function lineDistance(a: Vec, b: Vec, p: Vec): number {
  const l = dist(a, b) || 1;
  return Math.abs((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)) / l;
}

// ---- Mirror: a selection, then the line ---------------------------------------------------------------

class Mirror extends Action {
  readonly id = 'mirror';
  readonly name = t("Mirror");
  /** Whether the objects are chosen and the next click picks the line. */
  private line = false;
  private touched = false;
  override reset(): void { super.reset(); this.line = false; this.touched = false; }
  override get busy(): boolean { return this.line || this.touched; }
  hint(ctx: Context): string {
    if (this.line) return t("Click the mirror line");
    return ctx.selection.size ? t("Click more curves to mirror, or Enter, then click the mirror line") : t("Click the curves to mirror, then Enter");
  }
  override preview(ctx: Context, _c: Vec, hover: Hit): Preview {
    if (!this.line || !ctx.selection.size || entityOf(ctx.sketch, hover.entity)?.kind !== 'line') return none();
    return { ...none(), shapes: createdShapes(mirror(ctx.sketch, [...ctx.selection], hover.entity!)) };
  }
  click(ctx: Context, hit: Hit): Step {
    if (!this.line) {
      if (hit.entity === null) return { kind: 'more' };
      this.touched = true;
      return { kind: 'toggle', id: hit.entity };
    }
    if (entityOf(ctx.sketch, hit.entity)?.kind !== 'line') return { kind: 'error', message: t("Click a line to mirror across.") };
    const step = outcome(mirror(ctx.sketch, [...ctx.selection], hit.entity!), true);
    this.reset();
    return step;
  }
  override enter(ctx: Context): Step {
    if (!ctx.selection.size) return { kind: 'error', message: t("Click the curves to mirror first.") };
    this.line = true;
    return { kind: 'more' };
  }
}

// ---- Patterns -----------------------------------------------------------------------------------------

class RectPattern extends Action {
  readonly id = 'rpattern';
  readonly name = t("Rectangular pattern");
  private set = { nx: 3, ny: 2, dx: 0, dy: 0 };
  hint(ctx: Context): string { return ctx.selection.size ? t("Click more curves, set the counts and spacing · Enter to apply") : t("Click the curves to pattern"); }
  private values(ctx: Context, locked: Locked) {
    const b = boundsOf(ctx);
    const dx = this.set.dx || (b ? Math.round((b.maxX - b.minX + 5) * 100) / 100 : 10);
    const dy = this.set.dy || (b ? Math.round((b.maxY - b.minY + 5) * 100) / 100 : 10);
    return { nx: Math.max(1, Math.round(locked.nx ?? this.set.nx)), ny: Math.max(1, Math.round(locked.ny ?? this.set.ny)), dx: locked.dx ?? dx, dy: locked.dy ?? dy };
  }
  override fields(ctx: Context): Field[] {
    if (!ctx.selection.size) return [];
    const v = this.values(ctx, {});
    return [
      { key: 'nx', label: t("Columns"), kind: 'count', value: v.nx },
      { key: 'ny', label: t("Rows"), kind: 'count', value: v.ny },
      { key: 'dx', label: t("Spacing X"), kind: 'length', value: v.dx },
      { key: 'dy', label: t("Spacing Y"), kind: 'length', value: v.dy },
    ];
  }
  private run(ctx: Context, locked: Locked): Outcome {
    const v = this.values(ctx, locked);
    return rectangularPattern(ctx.sketch, [...ctx.selection], v.nx, v.ny, v.dx, v.dy);
  }
  override preview(ctx: Context, _c: Vec, _h: Hit, locked: Locked): Preview {
    if (!ctx.selection.size) return none();
    const v = this.values(ctx, locked);
    return v.nx * v.ny * ctx.selection.size > 3000 ? none() : { ...none(), shapes: createdShapes(this.run(ctx, locked)) };
  }
  click(_ctx: Context, hit: Hit): Step { return hit.entity === null ? { kind: 'more' } : { kind: 'toggle', id: hit.entity }; }
  override enter(ctx: Context, _c: Vec, locked: Locked): Step {
    if (!ctx.selection.size) return { kind: 'error', message: t("Select what to pattern first.") };
    this.set = this.values(ctx, locked);
    return outcome(this.run(ctx, locked), true);
  }
}

class CircPattern extends Action {
  readonly id = 'cpattern';
  readonly name = t("Circular pattern");
  private set = { count: 6, angle: 360 };
  hint(ctx: Context): string {
    if (!ctx.selection.size) return t("Click the curves to pattern");
    return this.picks.length ? t("Set the count and angle · Enter to apply") : t("Click more curves, or click the centre point of the pattern");
  }
  private values(locked: Locked) { return { count: Math.max(2, Math.round(locked.count ?? this.set.count)), angle: locked.angle ?? this.set.angle }; }
  override fields(ctx: Context): Field[] {
    if (!ctx.selection.size || !this.picks.length) return [];
    const v = this.values({});
    return [{ key: 'count', label: t("Count"), kind: 'count', value: v.count }, { key: 'angle', label: t("Angle"), kind: 'angle', value: v.angle }];
  }
  private center(ctx: Context): Vec { const h = this.picks[0]!; return h.point !== null ? point(ctx.sketch, h.point) : h.at; }
  override preview(ctx: Context, _c: Vec, _h: Hit, locked: Locked): Preview {
    if (!ctx.selection.size || !this.picks.length) return none();
    const v = this.values(locked);
    return { ...none(), shapes: createdShapes(circularPattern(ctx.sketch, [...ctx.selection], this.center(ctx), v.count, v.angle)) };
  }
  click(ctx: Context, hit: Hit, locked: Locked): Step {
    // Curves join the selection; a point, or empty space, is the centre.
    if (!this.picks.length && hit.point === null && hit.entity !== null) return { kind: 'toggle', id: hit.entity };
    if (!ctx.selection.size) return { kind: 'error', message: t("Click the curves to pattern first.") };
    if (!this.picks.length) { this.picks.push(hit); return { kind: 'more' }; }
    return this.enter(ctx, hit.at, locked);
  }
  override enter(ctx: Context, _c: Vec, locked: Locked): Step {
    if (!ctx.selection.size || !this.picks.length) return { kind: 'more' };
    this.set = this.values(locked);
    const step = outcome(circularPattern(ctx.sketch, [...ctx.selection], this.center(ctx), this.set.count, this.set.angle), true);
    this.reset();
    return step;
  }
}

function boundsOf(ctx: Context) {
  const s = clone(ctx.sketch);
  s.entities = s.entities.filter((e) => ctx.selection.has(e.id));
  return sketchBounds(s);
}

// ---- Constraints --------------------------------------------------------------------------------------

type Need = 'point' | 'line' | 'round' | 'curve' | 'any';

const fits = (s: Sketch, hit: Hit, need: Need): boolean => {
  if (need === 'point') return hit.point !== null;
  const e = entityOf(s, hit.entity);
  if (!e) return need === 'any' && hit.point !== null;
  if (need === 'line') return e.kind === 'line';
  if (need === 'round') return e.kind !== 'line';
  return true;
};

/** A curve picked where a point would do: lines and curves are what most relations take. */
const asCurve = (hit: Hit): Hit => ({ ...hit, point: null });

class Relate extends Action {
  constructor(readonly id: ConstraintId, readonly name: string) { super(); }

  private needs(ctx: Context): Need[] {
    const first = this.picks[0];
    switch (this.id) {
      case 'coincident': return ['any', 'any'];
      case 'collinear': case 'parallel': case 'perpendicular': return ['line', 'line'];
      case 'concentric': return ['round', 'round'];
      case 'midpoint': return first && first.point !== null ? ['point', 'line'] : ['line', 'point'];
      case 'fix': return ['any'];
      case 'hv': return ['line'];
      case 'tangent': return ['curve', 'curve'];
      case 'equal': return first && entityOf(ctx.sketch, first.entity)?.kind === 'line' ? ['line', 'line'] : ['curve', 'curve'];
      case 'symmetric': return ['any', 'any', 'line'];
    }
  }

  hint(ctx: Context): string {
    const n = this.picks.length;
    const what: Record<ConstraintId, string[]> = {
      coincident: ['Click a point', 'Click the point or curve it lies on'],
      collinear: ['Click a line', 'Click the line to be collinear with'],
      concentric: ['Click a circle or arc', 'Click the circle or arc to share its centre'],
      midpoint: ['Click a point or a line', 'Click the line or the point'],
      fix: ['Click a point or curve to fix or free it'],
      parallel: ['Click a line', 'Click the line to be parallel to'],
      perpendicular: ['Click a line', 'Click the line to be perpendicular to'],
      hv: ['Click a line to make it horizontal or vertical'],
      tangent: ['Click a curve', 'Click the curve to be tangent to'],
      equal: ['Click a line, circle or arc', 'Click the one to be equal to'],
      symmetric: ['Click the first point or curve', 'Click its mirror image', 'Click the symmetry line'],
    };
    void ctx;
    return what[this.id][n] ?? '';
  }

  click(ctx: Context, hit: Hit): Step {
    const needs = this.needs(ctx);
    const need = needs[this.picks.length]!;
    const pick = need === 'point' || need === 'any' ? hit : asCurve(hit);
    if (!fits(ctx.sketch, pick, need) || (pick.entity === null && pick.point === null)) return { kind: 'more' };
    this.picks.push(pick);
    if (this.picks.length < needs.length) return { kind: 'more' };
    const picks = this.picks;
    this.reset();
    const s = clone(ctx.sketch);
    const made = this.make(s, picks);
    return typeof made === 'string' ? { kind: 'error', message: made } : { kind: 'apply', sketch: s };
  }

  /** Adds the relation to `s`, or says why it cannot. */
  private make(s: Sketch, p: Hit[]): string | void {
    const [a, b, c] = p as [Hit, Hit, Hit];
    const add1 = (x: NewConstraint) => addConstraint(s, x);
    switch (this.id) {
      case 'coincident': {
        if (a.point !== null && b.point !== null) { if (a.point !== b.point) merge(s, a.point, b.point); return; }
        const pt = a.point ?? b.point, curve = a.point !== null ? b.entity : a.entity;
        if (pt === null || curve === null) return t("Pick a point and a point or curve.");
        add1({ kind: 'on', point: pt, curve }); return;
      }
      case 'collinear': case 'parallel': case 'perpendicular': case 'tangent': case 'concentric':
        if (a.entity === b.entity) return t("Pick two different curves.");
        if (this.id === 'tangent' && entityOf(s, a.entity)?.kind === 'line' && entityOf(s, b.entity)?.kind === 'line') return t("Two lines cannot be tangent.");
        add1({ kind: this.id, a: a.entity!, b: b.entity! }); return;
      case 'equal': {
        const ka = entityOf(s, a.entity)?.kind, kb = entityOf(s, b.entity)?.kind;
        if ((ka === 'line') !== (kb === 'line')) return t("Equal takes two lines, or two circles and arcs.");
        if (a.entity === b.entity) return t("Pick two different curves.");
        add1({ kind: 'equal', a: a.entity!, b: b.entity! }); return;
      }
      case 'midpoint': {
        const pt = a.point ?? b.point, line = a.point !== null ? b.entity : a.entity;
        add1({ kind: 'midpoint', point: pt!, line: line! }); return;
      }
      case 'hv': {
        const l = entityOf(s, a.entity);
        if (l?.kind !== 'line') return;
        const pa = point(s, l.a), pb = point(s, l.b);
        const kind = Math.abs(pb.x - pa.x) >= Math.abs(pb.y - pa.y) ? 'horizontal' : 'vertical';
        if (s.constraints.some((x) => (x.kind === 'horizontal' || x.kind === 'vertical') && x.line === l.id)) return t("That line is already horizontal or vertical.");
        add1({ kind, line: l.id }); return;
      }
      case 'fix': {
        const pts = a.point !== null ? [a.point] : (() => { const e = entityOf(s, a.entity); return e ? pointsOf(e) : []; })();
        const fixed = pts.every((id) => s.constraints.some((x) => x.kind === 'fix' && x.point === id));
        if (fixed) s.constraints = s.constraints.filter((x) => !(x.kind === 'fix' && pts.includes(x.point)));
        else for (const id of pts) if (!s.constraints.some((x) => x.kind === 'fix' && x.point === id)) add1({ kind: 'fix', point: id });
        return;
      }
      case 'symmetric': {
        const line = c.entity!;
        if (a.point !== null && b.point !== null) { add1({ kind: 'symmetric', a: a.point, b: b.point, line }); return; }
        const ea = entityOf(s, a.entity), eb = entityOf(s, b.entity);
        if (!ea || !eb || ea.kind !== eb.kind) return t("Symmetry takes two points, or two curves of the same kind.");
        if (ea.kind === 'line' && eb.kind === 'line') {
          // Each end pairs with the mirror end nearer to it.
          const [a1, a2] = [point(s, ea.a), point(s, ea.b)];
          const straight = dist(a1, point(s, eb.a)) + dist(a2, point(s, eb.b)) <= dist(a1, point(s, eb.b)) + dist(a2, point(s, eb.a));
          add1({ kind: 'symmetric', a: ea.a, b: straight ? eb.a : eb.b, line });
          add1({ kind: 'symmetric', a: ea.b, b: straight ? eb.b : eb.a, line });
          return;
        }
        if ('c' in ea && 'c' in eb) {
          add1({ kind: 'symmetric', a: ea.c, b: eb.c, line });
          add1({ kind: 'equal', a: ea.id, b: eb.id });
        }
        return;
      }
    }
  }
}

// ---- Sketch dimension ---------------------------------------------------------------------------------

/** Fusion's dimension: pick what to measure, then place the value and type it. */
class Dimensioning extends Action {
  readonly id = 'dimension';
  readonly name = t("Sketch dimension");
  private placing = false;
  override reset(): void { super.reset(); this.placing = false; }
  hint(): string {
    if (this.placing) return t("Place the dimension, type its value · Enter or click");
    if (!this.picks.length) return t("Click a line, circle, arc or point");
    return t("Click a second point, line, circle or arc, or click away to place the dimension");
  }

  /** The dimension the picks describe with its text at `at`, at the value it measures now. */
  private pending(s: Sketch, at: Vec): Dimension | null {
    const [a, b] = this.picks;
    if (!a) return null;
    const place: [number, number] = [at.x, at.y];
    const ea = entityOf(s, a.entity);
    const base = { id: -1, value: 0, at: place };
    let d: Dimension | null = null;
    if (!b) {
      if (ea?.kind === 'circle' && a.point === null) d = { ...base, kind: 'diameter', curve: ea.id };
      else if (ea?.kind === 'arc' && a.point === null) d = { ...base, kind: 'radius', curve: ea.id };
      else if (ea?.kind === 'line' && a.point === null) d = { ...base, kind: 'distance', a: ea.a, b: ea.b, axis: axisFor(point(s, ea.a), point(s, ea.b), at) };
    } else {
      // As Fusion: a point stands for itself, a circle or arc for its centre;
      // two lines give their distance when parallel and their angle when not.
      const eb = entityOf(s, b.entity);
      const spot = (h: Hit, e: Entity | undefined): Id | null => h.point ?? (e && 'c' in e ? e.c : null);
      const lineOf = (h: Hit, e: Entity | undefined) => (h.point === null && e?.kind === 'line' ? e : null);
      const pa = spot(a, ea), pb = spot(b, eb), la = lineOf(a, ea), lb = lineOf(b, eb);
      if (la && lb) {
        d = parallel(s, la, lb)
          ? { ...base, kind: 'offset', point: lb.a, line: la.id }
          : { ...base, kind: 'angle', a: la.id, b: lb.id };
      } else if (pa !== null && pb !== null) {
        if (pa !== pb) d = { ...base, kind: 'distance', a: pa, b: pb, axis: axisFor(point(s, pa), point(s, pb), at) };
      } else if (pa !== null && lb) d = { ...base, kind: 'offset', point: pa, line: lb.id };
      else if (pb !== null && la) d = { ...base, kind: 'offset', point: pb, line: la.id };
    }
    if (d) {
      d.value = measure(s, d);
      const o = labelAnchor(s, d);
      d.at = [at.x - o.x, at.y - o.y];
    }
    return d;
  }

  override fields(ctx: Context, cursor: Vec): Field[] {
    const d = this.pending(ctx.sketch, cursor);
    if (!d) return [];
    return [{ key: 'value', label: d.kind === 'angle' ? t("Angle") : d.kind === 'diameter' ? t("Diameter") : d.kind === 'radius' ? t("Radius") : t("Distance"), kind: d.kind === 'angle' ? 'angle' : 'length', value: d.value }];
  }

  override preview(ctx: Context, cursor: Vec): Preview {
    return { ...none(), dimension: this.pending(ctx.sketch, cursor) };
  }

  click(ctx: Context, hit: Hit, locked: Locked): Step {
    const s = ctx.sketch;
    if (this.placing) return this.place(s, hit.at, locked);
    if (!this.picks.length) {
      if (hit.point !== null) { this.picks.push(hit); return { kind: 'more' }; }
      if (entityOf(s, hit.entity)) this.picks.push(asCurve(hit));
      return { kind: 'more' };
    }
    const first = this.picks[0]!;
    const curve = entityOf(s, first.entity);
    const own = new Set(first.point !== null ? [first.point] : curve ? pointsOf(curve) : []);
    // A second point, or a second curve, makes a dimension between the two;
    // a curve's own point or empty space places the curve's own.
    if (hit.point !== null && !own.has(hit.point)) { this.picks.push(hit); this.placing = true; return { kind: 'more' }; }
    if (hit.point === null && hit.entity !== null && hit.entity !== first.entity) { this.picks.push(asCurve(hit)); this.placing = true; return { kind: 'more' }; }
    if (first.point !== null) return { kind: 'more' };
    return this.place(s, hit.at, locked);
  }

  override enter(ctx: Context, cursor: Vec, locked: Locked): Step {
    return this.picks.length ? this.place(ctx.sketch, cursor, locked) : { kind: 'more' };
  }

  private place(s0: Sketch, at: Vec, locked: Locked): Step {
    const d = this.pending(s0, at);
    this.reset();
    if (!d) return { kind: 'error', message: t("That cannot be dimensioned.") };
    const value = locked.value ?? d.value;
    if (!(value > 0) && d.kind !== 'angle') return { kind: 'error', message: t("A dimension must be above zero.") };
    const s = clone(s0);
    const { id: _drop, ...rest } = d;
    void _drop;
    const id = addConstraint(s, { ...rest, value } as never);
    // Placed without a typed value: its value is asked next, as in Fusion.
    return { kind: 'apply', sketch: s, ...(locked.value === undefined ? { edit: id } : {}) };
  }
}

/** Whether two lines run the same way, within half a degree. */
function parallel(s: Sketch, a: Entity & { kind: 'line' }, b: Entity & { kind: 'line' }): boolean {
  const pa = point(s, a.a), qa = point(s, a.b), pb = point(s, b.a), qb = point(s, b.b);
  const ua = { x: qa.x - pa.x, y: qa.y - pa.y }, ub = { x: qb.x - pb.x, y: qb.y - pb.y };
  const la = Math.hypot(ua.x, ua.y), lb = Math.hypot(ub.x, ub.y);
  if (!la || !lb) return false;
  return Math.abs(ua.x * ub.y - ua.y * ub.x) / (la * lb) < Math.sin(Math.PI / 360);
}

export function createAction(id: ActionId): Action {
  switch (id) {
    case 'trim': return new CurveClick('trim', t("Trim"), trim);
    case 'extend': return new CurveClick('extend', t("Extend"), extend);
    case 'break': return new CurveClick('break', t("Break"), breakAt);
    case 'fillet': return new Corner('fillet', t("Fillet"));
    case 'chamfer': return new Corner('chamfer', t("Chamfer"));
    case 'offset': return new Offset();
    case 'mirror': return new Mirror();
    case 'rpattern': return new RectPattern();
    case 'cpattern': return new CircPattern();
    case 'dimension': return new Dimensioning();
    default: return new Relate(id, id);
  }
}

