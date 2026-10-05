// A 2D sketch as Fusion keeps one: points shared between the curves that
// meet there, curves that refer to their points by id, and constraints
// over both. Coordinates are millimetres with Y up. A point two curves
// share is their coincidence; nothing else records it.

export type Id = number;

export interface Point {
  id: Id;
  x: number;
  y: number;
}

interface Curve {
  id: Id;
  /** Construction geometry guides the drawing and is never cut. */
  construction?: boolean;
}

export interface Line extends Curve { kind: 'line'; a: Id; b: Id }
export interface Circle extends Curve { kind: 'circle'; c: Id; r: number }
/** Counter-clockwise from `a` to `b` around `c`; both ends lie at the same radius. */
export interface Arc extends Curve { kind: 'arc'; c: Id; a: Id; b: Id }

export type Entity = Line | Circle | Arc;

/** Relations the solver keeps true. Tools record the ones Fusion adds while drawing. */
export type Constraint =
  | { id: Id; kind: 'horizontal' | 'vertical'; line: Id }
  | { id: Id; kind: 'parallel' | 'perpendicular' | 'equal'; a: Id; b: Id }
  | { id: Id; kind: 'midpoint'; point: Id; line: Id }
  /** The point lies on the curve: a line's infinite extension, or the full circle of an arc. */
  | { id: Id; kind: 'on'; point: Id; curve: Id };

export interface Sketch {
  points: Point[];
  entities: Entity[];
  constraints: Constraint[];
  /** The next unused id, shared by points, entities and constraints. */
  next: Id;
}

export const emptySketch = (): Sketch => ({ points: [], entities: [], constraints: [], next: 1 });

export function clone(sketch: Sketch): Sketch {
  return {
    points: sketch.points.map((p) => ({ ...p })),
    entities: sketch.entities.map((e) => ({ ...e })),
    constraints: sketch.constraints.map((c) => ({ ...c })),
    next: sketch.next,
  };
}

export function point(sketch: Sketch, id: Id): Point {
  const found = sketch.points.find((p) => p.id === id);
  if (!found) throw new Error(`sketch point ${id} is missing`);
  return found;
}

export function addPoint(sketch: Sketch, x: number, y: number): Id {
  const id = sketch.next++;
  sketch.points.push({ id, x, y });
  return id;
}

/** An existing point when `at` names one, otherwise a new point at x, y. */
export function pointAt(sketch: Sketch, x: number, y: number, at?: Id): Id {
  return at !== undefined && sketch.points.some((p) => p.id === at) ? at : addPoint(sketch, x, y);
}

type NewEntity = Line | Circle | Arc extends infer E ? (E extends Entity ? Omit<E, 'id'> : never) : never;
type NewConstraint = Constraint extends infer C ? (C extends Constraint ? Omit<C, 'id'> : never) : never;

export function addEntity(sketch: Sketch, entity: NewEntity): Id {
  const id = sketch.next++;
  sketch.entities.push({ ...entity, id } as Entity);
  return id;
}

export function addConstraint(sketch: Sketch, constraint: NewConstraint): Id {
  const id = sketch.next++;
  sketch.constraints.push({ ...constraint, id } as Constraint);
  return id;
}

/** The point ids an entity is drawn through. */
export function pointsOf(entity: Entity): Id[] {
  switch (entity.kind) {
    case 'line': return [entity.a, entity.b];
    case 'circle': return [entity.c];
    case 'arc': return [entity.c, entity.a, entity.b];
  }
}

function constraintRefs(c: Constraint): Id[] {
  switch (c.kind) {
    case 'horizontal': case 'vertical': return [c.line];
    case 'parallel': case 'perpendicular': case 'equal': return [c.a, c.b];
    case 'midpoint': return [c.point, c.line];
    case 'on': return [c.point, c.curve];
  }
}

/**
 * Removes entities, the constraints on them and the points nothing uses any
 * more. Deleting a point removes every curve drawn through it.
 */
export function remove(sketch: Sketch, ids: Iterable<Id>): void {
  const gone = new Set(ids);
  sketch.entities = sketch.entities.filter((e) => !gone.has(e.id) && !pointsOf(e).some((p) => gone.has(p)));
  const used = new Set(sketch.entities.flatMap(pointsOf));
  sketch.points = sketch.points.filter((p) => used.has(p.id));
  const alive = new Set([...used, ...sketch.entities.map((e) => e.id)]);
  sketch.constraints = sketch.constraints.filter((c) => !gone.has(c.id) && constraintRefs(c).every((r) => alive.has(r)));
}

/** Merges point `from` into `into`: every curve through `from` now passes through `into`. */
export function merge(sketch: Sketch, from: Id, into: Id): void {
  if (from === into) return;
  for (const e of sketch.entities) {
    if (e.kind === 'line') { if (e.a === from) e.a = into; if (e.b === from) e.b = into; }
    else if (e.kind === 'circle') { if (e.c === from) e.c = into; }
    else { if (e.c === from) e.c = into; if (e.a === from) e.a = into; if (e.b === from) e.b = into; }
  }
  for (const c of sketch.constraints) if ((c.kind === 'midpoint' || c.kind === 'on') && c.point === from) c.point = into;
  // A line whose ends merged has no length left.
  const collapsed = sketch.entities.filter((e) => e.kind === 'line' && e.a === e.b).map((e) => e.id);
  sketch.points = sketch.points.filter((p) => p.id !== from);
  if (collapsed.length) remove(sketch, collapsed);
}

/** Whether a stored sketch has the shape this module writes. */
export function isSketch(value: unknown): value is Sketch {
  if (!value || typeof value !== 'object') return false;
  const s = value as Partial<Sketch>;
  return Array.isArray(s.points) && Array.isArray(s.entities) && Array.isArray(s.constraints) && typeof s.next === 'number';
}
