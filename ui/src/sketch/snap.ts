// Where a click lands: the strongest feature within reach of the cursor,
// in Fusion's order. Existing points come first so a curve drawn from an
// end shares that point; then midpoints, crossings and the curves
// themselves; then alignment with the point the tool is drawing from;
// then the grid.
import { angleOf, dist, intersections, midOf, nearest, onSweep, shapeOf, type Shape, type Vec } from './geometry';
import type { Id, Sketch } from './model';

export type SnapKind = 'end' | 'center' | 'mid' | 'intersection' | 'on' | 'tangent' | 'horizontal' | 'vertical' | 'grid' | 'free';

export interface Snap extends Vec {
  /** Tangency inferred while drawing a line; contact may be at its start. */
  tangent?: { curve: Id; contact: Vec; at: 'start' | 'end' };
  kind: SnapKind;
  /** The existing point the click lands on. */
  point?: Id;
  /** The curve the click lands on. */
  entity?: Id;
}

export interface SnapOptions {
  /** Tangent inference applies to lines, not rectangle/circle radii or drags. */
  tangent?: boolean;
  /** Reach, in millimetres: a few screen pixels at the current zoom. */
  reach: number;
  /** The grid spacing in millimetres, or 0 for no grid snap. */
  grid: number;
  /** The point being drawn from, for horizontal and vertical alignment. */
  anchor?: Vec;
  /** Points and curves the tool is moving, which must not snap to themselves. */
  ignorePoints?: ReadonlySet<Id>;
  ignoreEntities?: ReadonlySet<Id>;
}

export function snap(sketch: Sketch, cursor: Vec, options: SnapOptions): Snap {
  const { reach, ignorePoints, ignoreEntities } = options;
  const centers = new Set(sketch.entities.flatMap((e) => ('c' in e ? [e.c] : [])));
  const curves = sketch.entities.filter((e) => !ignoreEntities?.has(e.id));
  const shapes: Array<{ id: Id; shape: Shape }> = curves.map((e) => ({ id: e.id, shape: shapeOf(sketch, e) }));

  const tangentSnaps: Snap[] = [];
  const anchor = options.anchor;
  if (options.tangent && anchor) for (const {id,shape} of shapes) {
    if (shape.kind === 'line' || shape.kind === 'poly' || !(shape.r > 1e-9)) continue;
    const dx=anchor.x-shape.c.x,dy=anchor.y-shape.c.y,d2=dx*dx+dy*dy;
    const onArc = (p: Vec) => shape.kind !== 'arc' || onSweep(angleOf(shape.c,p),shape.start,shape.end);
    if (Math.abs(Math.sqrt(d2)-shape.r)<1e-7 && onArc(anchor)) {
      const ux=-dy/shape.r,uy=dx/shape.r;
      const along=(cursor.x-anchor.x)*ux+(cursor.y-anchor.y)*uy;
      if (Math.abs(along)>reach) tangentSnaps.push({x:anchor.x+along*ux,y:anchor.y+along*uy,kind:'tangent',tangent:{curve:id,contact:{...anchor},at:'start'}});
    } else if (d2>shape.r*shape.r+1e-9) {
      const factor=shape.r*shape.r/d2,side=shape.r*Math.sqrt(d2-shape.r*shape.r)/d2;
      for (const sign of [-1,1]) {
        const contact={x:shape.c.x+factor*dx-sign*side*dy,y:shape.c.y+factor*dy+sign*side*dx};
        if (onArc(contact)) tangentSnaps.push({...contact,kind:'tangent',entity:id,tangent:{curve:id,contact,at:'end'}});
      }
    }
  }
  const enrich = (s: Snap): Snap => {
    const tangent=tangentSnaps.find(t=>dist(t,s)<1e-7);
    return tangent?{...s,kind:'tangent',tangent:tangent.tangent}:s;
  };
  let best: Snap | null = null;
  let bestDistance = reach;
  const offer = (s: Snap): void => {
    const d = dist(s, cursor);
    if (d <= bestDistance) { best = s; bestDistance = d; }
  };

  for (const p of sketch.points) {
    if (ignorePoints?.has(p.id)) continue;
    offer({ x: p.x, y: p.y, kind: centers.has(p.id) ? 'center' : 'end', point: p.id });
  }
  if (best) return enrich(best);

  // Tangent contact/direction beats an arbitrary on-curve or grid landing.
  for (const tangent of tangentSnaps) offer(tangent);
  if (best) return best;

  for (const { id, shape } of shapes) {
    const m = midOf(shape);
    if (m) offer({ ...m, kind: 'mid', entity: id });
  }
  if (best) return best;

  const near = shapes.filter(({ shape }) => dist(nearest(shape, cursor), cursor) <= reach * 2);
  for (let i = 0; i < near.length; i++) {
    for (let j = i + 1; j < near.length; j++) {
      for (const p of intersections(near[i]!.shape, near[j]!.shape)) offer({ ...p, kind: 'intersection' });
    }
  }
  if (best) return best;

  for (const { id, shape } of near) offer({ ...nearest(shape, cursor), kind: 'on', entity: id });
  if (best) return best;

  const grid = (v: number): number => (options.grid > 0 ? Math.round(v / options.grid) * options.grid : v);
  const kind: SnapKind = options.grid > 0 ? 'grid' : 'free';
  if (anchor) {
    const dx = Math.abs(cursor.x - anchor.x), dy = Math.abs(cursor.y - anchor.y);
    if (dx <= reach && dy > reach) return { x: anchor.x, y: grid(cursor.y), kind: 'vertical' };
    if (dy <= reach && dx > reach) return { x: grid(cursor.x), y: anchor.y, kind: 'horizontal' };
  }
  return { x: grid(cursor.x), y: grid(cursor.y), kind };
}

/** A grid spacing that keeps lines 12 px or more apart: 1, 2 or 5 times a power of ten. */
export function gridStep(mmPerPixel: number, minimumPixels = 12): number {
  const want = mmPerPixel * minimumPixels;
  const power = 10 ** Math.floor(Math.log10(want));
  for (const k of [1, 2, 5, 10]) if (k * power >= want) return k * power;
  return 10 * power;
}
