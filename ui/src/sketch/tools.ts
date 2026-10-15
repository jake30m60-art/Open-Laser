import { t } from '../i18n/index';
// The drawing tools. Each one walks through its clicks, shows a ghost of
// what the next click makes, and offers Fusion's heads-up fields: a typed
// value locks that measure, and the cursor supplies the rest. Lengths are
// millimetres and angles degrees, counter-clockwise from +X.
import {
  add, angleOf, circleThrough, cross, dist, dot, EPS, mid, onSweep, polar, scale, sub, type Shape, type Vec,
} from './geometry';
import { addConstraint, addEntity, addPoint, clampedKnots, pointAt, type Id, type Sketch } from './model';
import { splineAt } from './curves';
import type { Snap } from './snap';

export type ToolId = 'line' | 'rect2' | 'rectc' | 'circle' | 'circle2' | 'arc3' | 'arcc' | 'slot' | 'polygon' | 'ellipse' | 'spline';

/** One heads-up value: what the cursor gives now, or what was typed. */
export interface Field {
  key: string;
  label: string;
  kind: 'length' | 'angle' | 'count';
  value: number;
}

/** The values typed into the fields, by key. */
export type Locked = Partial<Record<string, number>>;

/** What the next click would make. */
export interface Ghost {
  shapes: Shape[];
  construction: Shape[];
  points: Vec[];
}

export type Outcome = 'more' | 'done' | 'ignored';

const DEG = Math.PI / 180;
const ghost = (shapes: Shape[] = [], construction: Shape[] = [], points: Vec[] = []): Ghost => ({ shapes, construction, points });
const line = (a: Vec, b: Vec): Shape => ({ kind: 'line', a, b });
const unit = (angle: number): Vec => ({ x: Math.cos(angle), y: Math.sin(angle) });
const signOr1 = (v: number): number => (v < 0 ? -1 : 1);

/** A position the locked values moved off the snapped feature keeps no reference to it. */
function moved(snap: Snap, to: Vec): Snap {
  return dist(snap, to) < 1e-9 ? snap : { x: to.x, y: to.y, kind: 'free' };
}

/** The sketch point for a snapped position, recording a midpoint or on-curve landing. */
function use(sketch: Sketch, s: Snap, automatic = false): Id {
  const id = pointAt(sketch, s.x, s.y, s.point);
  if (!automatic) return id;
  if (s.tangent?.at === 'end') {
    const curve=sketch.entities.find(e=>e.id===s.tangent!.curve);
    const arcEnd=curve?.kind==='arc' && (curve.a===id || curve.b===id);
    if (!arcEnd && !sketch.constraints.some(c=>c.kind==='on' && c.point===id && c.curve===s.tangent!.curve))
      addConstraint(sketch,{kind:'on',point:id,curve:s.tangent.curve});
    return id;
  }
  if (s.point === undefined && s.entity !== undefined) {
    if (s.kind === 'mid') addConstraint(sketch, { kind: 'midpoint', point: id, line: s.entity });
    else if (s.kind === 'on') addConstraint(sketch, { kind: 'on', point: id, curve: s.entity });
  }
  return id;
}

/** A line between two points, recording horizontal or vertical as Fusion infers it. */
function addLine(sketch: Sketch, a: Id, b: Id, from: Vec, to: Vec, construction = false, inferAxes = true): Id {
  const id = addEntity(sketch, { kind: 'line', a, b, ...(construction ? { construction } : {}) });
  if (!construction && inferAxes) {
    if (Math.abs(to.y - from.y) < 1e-9) addConstraint(sketch, { kind: 'horizontal', line: id });
    else if (Math.abs(to.x - from.x) < 1e-9) addConstraint(sketch, { kind: 'vertical', line: id });
  }
  return id;
}

/** A length and direction from `from`, either typed or taken from the cursor. */
function polarTarget(from: Vec, cursor: Snap, locked: Locked, lengthKey = 'length', angleKey = 'angle'): Snap {
  const L = locked[lengthKey], A = locked[angleKey];
  if (L === undefined && A === undefined) return cursor;
  const angle = A !== undefined ? A * DEG : dist(from, cursor) < EPS ? 0 : angleOf(from, cursor);
  const length = L !== undefined ? L : A !== undefined ? Math.max(0, dot(sub(cursor, from), unit(angle))) : dist(from, cursor);
  return moved(cursor, polar(from, length, angle));
}

function polarFields(from: Vec, to: Vec): Field[] {
  const angle = dist(from, to) < EPS ? 0 : angleOf(from, to) / DEG;
  return [
    { key: 'length', label: t("Length"), kind: 'length', value: dist(from, to) },
    { key: 'angle', label: t("Angle"), kind: 'angle', value: angle < 0 ? angle + 360 : angle },
  ];
}

export abstract class Tool {
  constructor(readonly automaticConstraints = false) {}
  abstract readonly id: ToolId;
  abstract readonly name: string;
  /** The clicks taken so far in the current shape. */
  protected picks: Snap[] = [];

  /** Whether a shape is under way; Escape abandons it before leaving the tool. */
  get busy(): boolean { return this.picks.length > 0; }
  reset(): void { this.picks = []; }
  /** Where alignment is measured from. */
  anchor(): Vec | undefined { return this.picks.at(-1); }
  /** Follows the cursor between clicks, for tools that track a direction. */
  move(_cursor: Vec): void {}
  /** The heads-up fields for the next click. */
  fields(_at: Snap): Field[] { return []; }
  /** Where the next click lands once the typed values apply. */
  resolve(cursor: Snap, _locked: Locked): Snap { return cursor; }
  abstract hint(): string;
  abstract ghost(at: Snap, locked: Locked): Ghost;
  /** Takes a click at a resolved position, adding to the sketch when a shape completes. */
  abstract click(sketch: Sketch, at: Snap, locked: Locked): Outcome;
}

/** A chain of lines; clicking the chain's first point closes it. */
class LineTool extends Tool {
  readonly id = 'line';
  readonly name = t("Line");
  hint(): string { return this.picks.length ? t("Pick the next point · click the start to close · Esc ends the chain") : t("Pick the start point"); }
  override fields(at: Snap): Field[] { const from = this.anchor(); return from ? polarFields(from, at) : []; }
  override resolve(cursor: Snap, locked: Locked): Snap {
    const from = this.anchor();
    if (!from) return cursor;
    const target = polarTarget(from, cursor, locked);
    if (cursor.tangent?.at === 'start') {
      const dx=cursor.x-from.x,dy=cursor.y-from.y,tx=target.x-from.x,ty=target.y-from.y;
      if (Math.hypot(tx,ty)>1e-9 && Math.abs(dx*ty-dy*tx)<1e-8*Math.hypot(dx,dy)*Math.hypot(tx,ty))
        return {...target,kind:'tangent',tangent:cursor.tangent};
    }
    return target;
  }
  ghost(at: Snap): Ghost { const from = this.anchor(); return from ? ghost([line(from, at)]) : ghost(); }
  click(sketch: Sketch, at: Snap): Outcome {
    const from = this.picks.at(-1);
    if (!from) { this.picks.push(at); return 'more'; }
    if (dist(from, at) < EPS) return 'ignored';
    const first = this.picks[0]!;
    const a = use(sketch, from, this.automaticConstraints);
    if (from === first) this.picks[0] = { ...first, point: a, kind: 'end' };
    const closes = this.picks.length > 1 && (at.point !== undefined ? at.point === this.picks[0]!.point : dist(at, first) < EPS);
    const b = closes ? this.picks[0]!.point! : use(sketch, at, this.automaticConstraints);
    const id = addLine(sketch, a, b, from, at, false, !at.tangent);
    if (this.automaticConstraints && at.tangent) addConstraint(sketch, { kind: 'tangent', a: id, b: at.tangent.curve });
    if (closes) { this.reset(); return 'done'; }
    this.picks.push({ x: at.x, y: at.y, kind: 'end', point: b });
    return 'more';
  }
}

function rectangle(sketch: Sketch, corners: [Vec, Vec, Vec, Vec], ids: [Id, Id, Id, Id]): void {
  for (let i = 0; i < 4; i++) addLine(sketch, ids[i]!, ids[(i + 1) % 4]!, corners[i]!, corners[(i + 1) % 4]!);
}

/** A typed width and height stay as dimensions, as Fusion keeps them: the
 *  rectangle then holds its size and moves whole when it is dimensioned to
 *  something else. */
function typedSize(sketch: Sketch, corners: [Vec, Vec, Vec, Vec], ids: [Id, Id, Id, Id], locked: Locked): void {
  const [a, b, c] = corners;
  if (locked.width !== undefined) {
    addConstraint(sketch, { kind: 'distance', a: ids[0], b: ids[1], axis: 'horizontal', value: locked.width, at: [(a.x + b.x) / 2, Math.min(a.y, c.y) - 8] });
  }
  if (locked.height !== undefined) {
    addConstraint(sketch, { kind: 'distance', a: ids[1], b: ids[2], axis: 'vertical', value: locked.height, at: [Math.max(a.x, b.x) + 8, (b.y + c.y) / 2] });
  }
}

const rectShapes = (c: [Vec, Vec, Vec, Vec]): Shape[] => c.map((p, i) => line(p, c[(i + 1) % 4]!));

/** Two opposite corners. */
class Rect2Tool extends Tool {
  readonly id = 'rect2';
  readonly name = t("2-point rectangle");
  hint(): string { return this.picks.length ? t("Pick the opposite corner") : t("Pick the first corner"); }
  override fields(at: Snap): Field[] {
    const a = this.anchor();
    return a ? [{ key: 'width', label: t("Width"), kind: 'length', value: Math.abs(at.x - a.x) }, { key: 'height', label: t("Height"), kind: 'length', value: Math.abs(at.y - a.y) }] : [];
  }
  override resolve(cursor: Snap, locked: Locked): Snap {
    const a = this.anchor();
    if (!a) return cursor;
    const x = locked.width !== undefined ? a.x + signOr1(cursor.x - a.x) * locked.width : cursor.x;
    const y = locked.height !== undefined ? a.y + signOr1(cursor.y - a.y) * locked.height : cursor.y;
    return moved(cursor, { x, y });
  }
  private corners(a: Vec, c: Vec): [Vec, Vec, Vec, Vec] { return [a, { x: c.x, y: a.y }, c, { x: a.x, y: c.y }]; }
  ghost(at: Snap): Ghost { const a = this.anchor(); return a ? ghost(rectShapes(this.corners(a, at))) : ghost(); }
  click(sketch: Sketch, at: Snap, locked: Locked = {}): Outcome {
    const a = this.picks[0];
    if (!a) { this.picks.push(at); return 'more'; }
    if (Math.abs(at.x - a.x) < EPS || Math.abs(at.y - a.y) < EPS) return 'ignored';
    const corners = this.corners(a, at);
    const ids: [Id, Id, Id, Id] = [use(sketch, a, this.automaticConstraints), addPoint(sketch, corners[1].x, corners[1].y), use(sketch, at, this.automaticConstraints), addPoint(sketch, corners[3].x, corners[3].y)];
    rectangle(sketch, corners, ids);
    typedSize(sketch, corners, ids, locked);
    this.reset();
    return 'done';
  }
}

/** A centre and a corner; construction diagonals keep the centre. */
class RectCTool extends Tool {
  readonly id = 'rectc';
  readonly name = t("Center rectangle");
  hint(): string { return this.picks.length ? t("Pick a corner") : t("Pick the center"); }
  override fields(at: Snap): Field[] {
    const c = this.anchor();
    return c ? [{ key: 'width', label: t("Width"), kind: 'length', value: 2 * Math.abs(at.x - c.x) }, { key: 'height', label: t("Height"), kind: 'length', value: 2 * Math.abs(at.y - c.y) }] : [];
  }
  override resolve(cursor: Snap, locked: Locked): Snap {
    const c = this.anchor();
    if (!c) return cursor;
    const x = locked.width !== undefined ? c.x + signOr1(cursor.x - c.x) * locked.width / 2 : cursor.x;
    const y = locked.height !== undefined ? c.y + signOr1(cursor.y - c.y) * locked.height / 2 : cursor.y;
    return moved(cursor, { x, y });
  }
  private corners(c: Vec, p: Vec): [Vec, Vec, Vec, Vec] {
    const dx = Math.abs(p.x - c.x), dy = Math.abs(p.y - c.y);
    return [{ x: c.x - dx, y: c.y - dy }, { x: c.x + dx, y: c.y - dy }, { x: c.x + dx, y: c.y + dy }, { x: c.x - dx, y: c.y + dy }];
  }
  ghost(at: Snap): Ghost {
    const c = this.anchor();
    if (!c) return ghost();
    const k = this.corners(c, at);
    return ghost(rectShapes(k), [line(k[0], k[2]), line(k[1], k[3])], [c]);
  }
  click(sketch: Sketch, at: Snap, locked: Locked = {}): Outcome {
    const c = this.picks[0];
    if (!c) { this.picks.push(at); return 'more'; }
    if (Math.abs(at.x - c.x) < EPS || Math.abs(at.y - c.y) < EPS) return 'ignored';
    const k = this.corners(c, at);
    const ids = k.map((p) => addPoint(sketch, p.x, p.y)) as [Id, Id, Id, Id];
    rectangle(sketch, k, ids);
    typedSize(sketch, k, ids, locked);
    const center = use(sketch, c, this.automaticConstraints);
    const diagonal = addLine(sketch, ids[0], ids[2], k[0], k[2], true);
    addLine(sketch, ids[1], ids[3], k[1], k[3], true);
    addConstraint(sketch, { kind: 'midpoint', point: center, line: diagonal });
    this.reset();
    return 'done';
  }
}

/** A centre and a diameter. */
class CircleTool extends Tool {
  readonly id = 'circle';
  readonly name = t("Center diameter circle");
  hint(): string { return this.picks.length ? t("Set the diameter") : t("Pick the center"); }
  override fields(at: Snap): Field[] { const c = this.anchor(); return c ? [{ key: 'diameter', label: t("Diameter"), kind: 'length', value: 2 * dist(c, at) }] : []; }
  override resolve(cursor: Snap, locked: Locked): Snap {
    const c = this.anchor();
    if (!c || locked.diameter === undefined) return cursor;
    const angle = dist(c, cursor) < EPS ? 0 : angleOf(c, cursor);
    return moved(cursor, polar(c, locked.diameter / 2, angle));
  }
  ghost(at: Snap): Ghost { const c = this.anchor(); return c ? ghost([{ kind: 'circle', c, r: dist(c, at) }], [], [c]) : ghost(); }
  click(sketch: Sketch, at: Snap, locked: Locked = {}): Outcome {
    const c = this.picks[0];
    if (!c) { this.picks.push(at); return 'more'; }
    const r = dist(c, at);
    if (r < EPS) return 'ignored';
    const circle = addEntity(sketch, { kind: 'circle', c: use(sketch, c, this.automaticConstraints), r });
    // A typed diameter stays as a dimension, as Fusion keeps it.
    if (locked.diameter !== undefined) addConstraint(sketch, { kind: 'diameter', curve: circle, value: locked.diameter, at: [c.x + r * 0.7, c.y + r * 0.7] });
    this.reset();
    return 'done';
  }
}

/** Two ends of a diameter. */
class Circle2Tool extends Tool {
  readonly id = 'circle2';
  readonly name = t("2-point circle");
  hint(): string { return this.picks.length ? t("Pick the other end of the diameter") : t("Pick the first point on the circle"); }
  override fields(at: Snap): Field[] { const a = this.anchor(); return a ? polarFields(a, at).map((f) => (f.key === 'length' ? { ...f, key: 'diameter', label: t("Diameter") } : f)) : []; }
  override resolve(cursor: Snap, locked: Locked): Snap { const a = this.anchor(); return a ? polarTarget(a, cursor, locked, 'diameter') : cursor; }
  ghost(at: Snap): Ghost { const a = this.anchor(); return a ? ghost([{ kind: 'circle', c: mid(a, at), r: dist(a, at) / 2 }], [line(a, at)]) : ghost(); }
  click(sketch: Sketch, at: Snap): Outcome {
    const a = this.picks[0];
    if (!a) { this.picks.push(at); return 'more'; }
    const r = dist(a, at) / 2;
    if (r < EPS) return 'ignored';
    const c = mid(a, at);
    addEntity(sketch, { kind: 'circle', c: addPoint(sketch, c.x, c.y), r });
    this.reset();
    return 'done';
  }
}

/** The arc from `s` to `e` through `p`, oriented counter-clockwise. */
function arcThrough(s: Vec, e: Vec, p: Vec): { c: Vec; r: number; a: 's' | 'e'; shape: Shape } | null {
  const circle = circleThrough(s, p, e);
  if (!circle) return null;
  const as = angleOf(circle.c, s), ae = angleOf(circle.c, e);
  const forward = onSweep(angleOf(circle.c, p), as, ae);
  const start = forward ? as : ae, end = forward ? ae : as;
  return { c: circle.c, r: circle.r, a: forward ? 's' : 'e', shape: { kind: 'arc', c: circle.c, r: circle.r, start, end } };
}

/** Start, end, then a point the arc passes through. */
class Arc3Tool extends Tool {
  readonly id = 'arc3';
  readonly name = t("3-point arc");
  hint(): string { return ['Pick the start point', 'Pick the end point', 'Pick a point on the arc'][this.picks.length] ?? ''; }
  override fields(at: Snap): Field[] {
    if (this.picks.length !== 2) return this.picks.length === 1 ? polarFields(this.picks[0]!, at).slice(0, 1).map((f) => ({ ...f, label: t("Chord") })) : [];
    const arc = arcThrough(this.picks[0]!, this.picks[1]!, at);
    return [{ key: 'radius', label: t("Radius"), kind: 'length', value: arc?.r ?? 0 }];
  }
  override resolve(cursor: Snap, locked: Locked): Snap {
    if (this.picks.length === 1) return polarTarget(this.picks[0]!, cursor, locked);
    if (this.picks.length !== 2 || locked.radius === undefined) return cursor;
    const [s, e] = [this.picks[0]!, this.picks[1]!];
    const half = dist(s, e) / 2;
    const r = Math.max(locked.radius, half);
    const chord = sub(e, s);
    const n = scale({ x: -chord.y, y: chord.x }, 1 / (2 * half || 1));
    const side = signOr1(cross(chord, sub(cursor, s)));
    const h = Math.sqrt(Math.max(0, r * r - half * half));
    return moved(cursor, add(mid(s, e), scale(n, side * (r - h))));
  }
  ghost(at: Snap): Ghost {
    if (this.picks.length === 1) return ghost([], [line(this.picks[0]!, at)]);
    if (this.picks.length !== 2) return ghost();
    const arc = arcThrough(this.picks[0]!, this.picks[1]!, at);
    return arc ? ghost([arc.shape], [], [arc.c]) : ghost([], [line(this.picks[0]!, this.picks[1]!)]);
  }
  click(sketch: Sketch, at: Snap): Outcome {
    if (this.picks.length < 2) {
      if (this.picks.length === 1 && dist(this.picks[0]!, at) < EPS) return 'ignored';
      this.picks.push(at);
      return 'more';
    }
    const [s, e] = [this.picks[0]!, this.picks[1]!];
    const arc = arcThrough(s, e, at);
    if (!arc) return 'ignored';
    const sId = use(sketch, s, this.automaticConstraints), eId = use(sketch, e, this.automaticConstraints);
    const c = addPoint(sketch, arc.c.x, arc.c.y);
    addEntity(sketch, { kind: 'arc', c, a: arc.a === 's' ? sId : eId, b: arc.a === 's' ? eId : sId });
    this.reset();
    return 'done';
  }
}

/** A centre, a start point and a sweep that follows the cursor's direction. */
class ArcCTool extends Tool {
  readonly id = 'arcc';
  readonly name = t("Center point arc");
  /** The signed sweep the cursor has wound so far, and the last angle it was at. */
  private wound = 0;
  private last: number | null = null;
  hint(): string { return ['Pick the center', 'Pick the start point', 'Pick the end point'][this.picks.length] ?? ''; }
  override reset(): void { super.reset(); this.wound = 0; this.last = null; }
  override move(cursor: Vec): void {
    if (this.picks.length !== 2) return;
    const c = this.picks[0]!;
    const angle = angleOf(c, cursor);
    if (this.last === null) this.last = angleOf(c, this.picks[1]!);
    let delta = angle - this.last;
    if (delta > Math.PI) delta -= 2 * Math.PI;
    if (delta < -Math.PI) delta += 2 * Math.PI;
    this.wound = Math.max(-2 * Math.PI + 1e-6, Math.min(2 * Math.PI - 1e-6, this.wound + delta));
    this.last = angle;
  }
  private signedSweep(locked: Locked): number {
    return locked.angle !== undefined ? signOr1(this.wound) * Math.min(359.999, Math.abs(locked.angle)) * DEG : this.wound;
  }
  override fields(at: Snap): Field[] {
    if (this.picks.length === 1) return [{ key: 'radius', label: t("Radius"), kind: 'length', value: dist(this.picks[0]!, at) }];
    if (this.picks.length === 2) return [{ key: 'angle', label: t("Angle"), kind: 'angle', value: Math.abs(this.wound) / DEG }];
    return [];
  }
  override resolve(cursor: Snap, locked: Locked): Snap {
    const c = this.picks[0];
    if (!c) return cursor;
    if (this.picks.length === 1) {
      if (locked.radius === undefined) return cursor;
      return moved(cursor, polar(c, locked.radius, dist(c, cursor) < EPS ? 0 : angleOf(c, cursor)));
    }
    const s = this.picks[1]!;
    return moved(cursor, polar(c, dist(c, s), angleOf(c, s) + this.signedSweep(locked)));
  }
  ghost(at: Snap, locked: Locked): Ghost {
    const c = this.picks[0];
    if (!c) return ghost();
    if (this.picks.length === 1) return ghost([], [{ kind: 'circle', c, r: dist(c, at) }, line(c, at)], [c]);
    const s = this.picks[1]!;
    const r = dist(c, s), start = angleOf(c, s), sw = this.signedSweep(locked);
    const shape: Shape = sw >= 0 ? { kind: 'arc', c, r, start, end: start + sw } : { kind: 'arc', c, r, start: start + sw, end: start };
    return ghost([shape], [line(c, s), line(c, at)], [c]);
  }
  click(sketch: Sketch, at: Snap, locked: Locked): Outcome {
    if (this.picks.length === 0) { this.picks.push(at); return 'more'; }
    if (this.picks.length === 1) {
      if (dist(this.picks[0]!, at) < EPS) return 'ignored';
      this.picks.push(at);
      this.wound = 0;
      this.last = null;
      return 'more';
    }
    const sw = this.signedSweep(locked);
    if (Math.abs(sw) < 1e-6) return 'ignored';
    const [c, s] = [this.picks[0]!, this.picks[1]!];
    const cId = use(sketch, c, this.automaticConstraints), sId = use(sketch, s, this.automaticConstraints);
    const eId = dist(at, polar(c, dist(c, s), angleOf(c, at))) < 1e-9 && at.point !== undefined ? at.point : addPoint(sketch, at.x, at.y);
    addEntity(sketch, sw > 0 ? { kind: 'arc', c: cId, a: sId, b: eId } : { kind: 'arc', c: cId, a: eId, b: sId });
    this.reset();
    return 'done';
  }
}

/** Two centres, then the width: two parallel lines capped by half circles. */
class SlotTool extends Tool {
  readonly id = 'slot';
  readonly name = t("Center to center slot");
  hint(): string { return ['Pick the first center', 'Pick the second center', 'Set the width'][this.picks.length] ?? ''; }
  private width(at: Vec, locked: Locked): number {
    if (locked.width !== undefined) return locked.width;
    const [a, b] = [this.picks[0]!, this.picks[1]!];
    const d = sub(b, a);
    return (2 * Math.abs(cross(d, sub(at, a)))) / (Math.hypot(d.x, d.y) || 1);
  }
  override fields(at: Snap): Field[] {
    if (this.picks.length === 1) return polarFields(this.picks[0]!, at);
    if (this.picks.length === 2) return [{ key: 'width', label: t("Width"), kind: 'length', value: this.width(at, {}) }];
    return [];
  }
  override resolve(cursor: Snap, locked: Locked): Snap { return this.picks.length === 1 ? polarTarget(this.picks[0]!, cursor, locked) : cursor; }
  private outline(a: Vec, b: Vec, w: number): { p: [Vec, Vec, Vec, Vec]; shapes: Shape[] } {
    const u = scale(sub(b, a), 1 / (dist(a, b) || 1));
    const n = { x: -u.y, y: u.x };
    const h = w / 2;
    const p: [Vec, Vec, Vec, Vec] = [add(a, scale(n, h)), add(b, scale(n, h)), sub(b, scale(n, h)), sub(a, scale(n, h))];
    const up = angleOf({ x: 0, y: 0 }, n);
    return {
      p,
      shapes: [line(p[0], p[1]), line(p[2], p[3]), { kind: 'arc', c: b, r: h, start: up - Math.PI, end: up }, { kind: 'arc', c: a, r: h, start: up, end: up + Math.PI }],
    };
  }
  ghost(at: Snap, locked: Locked): Ghost {
    if (this.picks.length === 1) return ghost([], [line(this.picks[0]!, at)], [this.picks[0]!]);
    if (this.picks.length !== 2) return ghost();
    const [a, b] = [this.picks[0]!, this.picks[1]!];
    return ghost(this.outline(a, b, this.width(at, locked)).shapes, [line(a, b)], [a, b]);
  }
  click(sketch: Sketch, at: Snap, locked: Locked): Outcome {
    if (this.picks.length < 2) {
      if (this.picks.length === 1 && dist(this.picks[0]!, at) < EPS) return 'ignored';
      this.picks.push(at);
      return 'more';
    }
    const w = this.width(at, locked);
    if (w < EPS) return 'ignored';
    const [a, b] = [this.picks[0]!, this.picks[1]!];
    const { p } = this.outline(a, b, w);
    const ca = use(sketch, a, this.automaticConstraints), cb = use(sketch, b, this.automaticConstraints);
    const ids = p.map((q) => addPoint(sketch, q.x, q.y)) as [Id, Id, Id, Id];
    const top = addLine(sketch, ids[0], ids[1], p[0], p[1]);
    const bottom = addLine(sketch, ids[2], ids[3], p[2], p[3]);
    const capB = addEntity(sketch, { kind: 'arc', c: cb, a: ids[2], b: ids[1] });
    const capA = addEntity(sketch, { kind: 'arc', c: ca, a: ids[0], b: ids[3] });
    addLine(sketch, ca, cb, a, b, true);
    // As FreeCAD draws a slot: each cap tangent to both sides, which makes them equal too.
    for (const cap of [capA, capB]) for (const side of [top, bottom]) addConstraint(sketch, { kind: 'tangent', a: side, b: cap });
    this.reset();
    return 'done';
  }
}

/** A regular polygon inscribed in a circle: centre, then a corner. */
class PolygonTool extends Tool {
  readonly id = 'polygon';
  readonly name = t("Inscribed polygon");
  private sides = 6;
  hint(): string { return this.picks.length ? t("Pick a corner · Tab to the side count") : t("Pick the center"); }
  private count(locked: Locked): number {
    const n = locked.sides === undefined ? this.sides : Math.round(locked.sides);
    return Math.max(3, Math.min(64, n));
  }
  override fields(at: Snap): Field[] {
    const c = this.anchor();
    return c ? [{ key: 'sides', label: t("Sides"), kind: 'count', value: this.sides }, { key: 'diameter', label: t("Diameter"), kind: 'length', value: 2 * dist(c, at) }] : [];
  }
  override resolve(cursor: Snap, locked: Locked): Snap {
    const c = this.anchor();
    if (locked.sides !== undefined) this.sides = this.count(locked);
    if (!c || locked.diameter === undefined) return cursor;
    return moved(cursor, polar(c, locked.diameter / 2, dist(c, cursor) < EPS ? 0 : angleOf(c, cursor)));
  }
  private corners(c: Vec, at: Vec, n: number): Vec[] {
    const r = dist(c, at), start = angleOf(c, at);
    return Array.from({ length: n }, (_, i) => polar(c, r, start + (2 * Math.PI * i) / n));
  }
  ghost(at: Snap, locked: Locked): Ghost {
    const c = this.anchor();
    if (!c) return ghost();
    const k = this.corners(c, at, this.count(locked));
    return ghost(k.map((p, i) => line(p, k[(i + 1) % k.length]!)), [{ kind: 'circle', c, r: dist(c, at) }], [c]);
  }
  click(sketch: Sketch, at: Snap, locked: Locked): Outcome {
    const c = this.picks[0];
    if (!c) { this.picks.push(at); return 'more'; }
    if (dist(c, at) < EPS) return 'ignored';
    const k = this.corners(c, at, this.count(locked));
    const ids = k.map((p, i) => (i === 0 ? use(sketch, at, this.automaticConstraints) : addPoint(sketch, p.x, p.y)));
    // One side may take an axis; on the circle the others would repeat it.
    const lines = k.map((p, i) => addLine(sketch, ids[i]!, ids[(i + 1) % k.length]!, p, k[(i + 1) % k.length]!, false, i === 0));
    for (const other of lines.slice(1)) addConstraint(sketch, { kind: 'equal', a: lines[0]!, b: other });
    // As FreeCAD draws one: every corner on a construction circle, so equal
    // sides keep the polygon regular when a side or the circle is dimensioned.
    const centre = use(sketch, c, this.automaticConstraints);
    const circle = addEntity(sketch, { kind: 'circle', c: centre, r: dist(c, at), construction: true });
    for (const id of ids) addConstraint(sketch, { kind: 'on', point: id, curve: circle });
    this.reset();
    return 'done';
  }
}

/** An ellipse: its centre, the end of its major axis, then how wide it is. */
class EllipseTool extends Tool {
  readonly id = 'ellipse';
  readonly name = t("Ellipse");
  hint(): string {
    return this.picks.length === 0 ? t("Pick the center") : this.picks.length === 1 ? t("Pick the end of the major axis") : t("Set the minor radius");
  }
  /** The minor radius a cursor gives: its distance from the major axis. */
  private minor(c: Vec, m: Vec, at: Vec): number {
    const u = sub(m, c), l = Math.hypot(u.x, u.y);
    return l < EPS ? 0 : Math.abs(cross(u, sub(at, c))) / l;
  }
  override fields(at: Snap): Field[] {
    const [c, m] = this.picks;
    if (!c) return [];
    if (!m) return polarFields(c, at).map((f) => (f.key === 'length' ? { ...f, key: 'major', label: t("Major radius") } : f));
    return [{ key: 'minor', label: t("Minor radius"), kind: 'length', value: this.minor(c, m, at) }];
  }
  override resolve(cursor: Snap, locked: Locked): Snap {
    const [c, m] = this.picks;
    if (!c) return cursor;
    if (!m) return polarTarget(c, cursor, locked, 'major');
    if (locked.minor === undefined) return cursor;
    const u = sub(m, c), l = Math.hypot(u.x, u.y) || 1;
    const n = { x: -u.y / l, y: u.x / l }, side = signOr1(dot(sub(cursor, c), n));
    return moved(cursor, add(c, scale(n, side * locked.minor)));
  }
  private points(c: Vec, m: Vec, r: number): Vec[] {
    const u = sub(m, c), l = Math.hypot(u.x, u.y) || 1, v = { x: (-u.y / l) * r, y: (u.x / l) * r };
    return Array.from({ length: 97 }, (_, i) => { const a = (i / 96) * 2 * Math.PI; return add(c, add(scale(u, Math.cos(a)), scale(v, Math.sin(a)))); });
  }
  ghost(at: Snap): Ghost {
    const [c, m] = this.picks;
    if (!c) return ghost();
    if (!m) return ghost([], [line(c, at)], [c]);
    return ghost([{ kind: 'poly', pts: this.points(c, m, this.minor(c, m, at)), closed: true }], [line(c, m)], [c, m]);
  }
  click(sketch: Sketch, at: Snap): Outcome {
    const [c, m] = this.picks;
    if (!c) { this.picks.push(at); return 'more'; }
    if (!m) { if (dist(c, at) < EPS) return 'ignored'; this.picks.push(at); return 'more'; }
    const r = this.minor(c, m, at);
    if (r < EPS) return 'ignored';
    // The longer axis is the major one: a wide ellipse turns its axes.
    const a = dist(c, m);
    const u = sub(m, c), side = signOr1(cross(u, sub(at, c)));
    const major = r <= a ? m : add(c, scale({ x: -u.y, y: u.x }, (side * r) / a));
    const centre = use(sketch, c, this.automaticConstraints);
    addEntity(sketch, { kind: 'ellipse', c: centre, m: r <= a ? use(sketch, m, this.automaticConstraints) : addPoint(sketch, major.x, major.y), r: Math.min(r, a) });
    this.reset();
    return 'done';
  }
}

/** A spline through control points: each click adds a pole, clicking the
 *  first pole closes it, Esc ends it. It runs through its first and last pole. */
class SplineTool extends Tool {
  readonly id = 'spline';
  readonly name = t("Control point spline");
  /** The spline being drawn and its first pole, once it has two. */
  private drawing: { id: Id; first: Id } | null = null;
  override reset(): void { super.reset(); this.drawing = null; }
  hint(): string { return this.picks.length ? t("Pick the next control point · click the first to close · Esc ends the spline") : t("Pick the first control point"); }
  override fields(at: Snap): Field[] { const from = this.anchor(); return from ? polarFields(from, at) : []; }
  override resolve(cursor: Snap, locked: Locked): Snap { const from = this.anchor(); return from ? polarTarget(from, cursor, locked) : cursor; }
  ghost(at: Snap): Ghost {
    if (!this.picks.length) return ghost();
    const poles = [...this.picks, at];
    const degree = Math.min(3, poles.length - 1);
    const knots = clampedKnots(poles.length, degree), top = knots[poles.length]!;
    const pts = Array.from({ length: 121 }, (_, i) => splineAt(poles, degree, knots, undefined, (top * i) / 120));
    return ghost([{ kind: 'poly', pts, closed: false }], poles.slice(1).map((p, i) => line(poles[i]!, p)), poles);
  }
  click(sketch: Sketch, at: Snap): Outcome {
    const spline = this.drawing && sketch.entities.find((e) => e.id === this.drawing!.id);
    if (this.picks.length && !(spline && spline.kind === 'spline') && this.drawing) this.reset();
    if (!this.picks.length) { this.picks.push(at); return 'more'; }
    if (dist(this.anchor()!, at) < EPS) return 'ignored';
    const closing = !!this.drawing && at.point === this.drawing.first;
    const pole = closing ? this.drawing!.first : use(sketch, at, this.automaticConstraints);
    if (!this.drawing || !spline || spline.kind !== 'spline') {
      const first = use(sketch, this.picks[0]!, this.automaticConstraints);
      const id = addEntity(sketch, { kind: 'spline', poles: [first, pole], degree: 1, knots: clampedKnots(2, 1) });
      this.drawing = { id, first };
    } else {
      spline.poles.push(pole);
      spline.degree = Math.min(3, spline.poles.length - 1);
      spline.knots = clampedKnots(spline.poles.length, spline.degree);
    }
    this.picks.push(at);
    if (closing) { this.reset(); return 'done'; }
    return 'more';
  }
}


export function createTool(id: ToolId, automaticConstraints = false): Tool {
  switch (id) {
    case 'line': return new LineTool(automaticConstraints);
    case 'rect2': return new Rect2Tool(automaticConstraints);
    case 'rectc': return new RectCTool(automaticConstraints);
    case 'circle': return new CircleTool(automaticConstraints);
    case 'circle2': return new Circle2Tool(automaticConstraints);
    case 'arc3': return new Arc3Tool(automaticConstraints);
    case 'arcc': return new ArcCTool(automaticConstraints);
    case 'slot': return new SlotTool(automaticConstraints);
    case 'polygon': return new PolygonTool(automaticConstraints);
    case 'ellipse': return new EllipseTool(automaticConstraints);
    case 'spline': return new SplineTool(automaticConstraints);
  }
}

