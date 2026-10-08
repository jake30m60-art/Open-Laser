import { fromDrawing, type SourceDrawing } from './from-drawing';
import type { Entity, Sketch } from './model';

/** Adds another sketch's points and curves under new ids; their ids. */
export function appendSketch(target: Sketch, source: Sketch): number[] {
  const offset = target.next - 1;
  target.points.push(...source.points.map((p) => ({ ...p, id: p.id + offset })));
  const shift = (e: Entity): Entity => {
    const x = { ...e, id: e.id + offset } as Record<string, unknown>;
    for (const k of ['a', 'b', 'c', 'm']) if (typeof x[k] === 'number') x[k] = (x[k] as number) + offset;
    if (Array.isArray(x.poles)) x.poles = (x.poles as number[]).map((p) => p + offset);
    return x as unknown as Entity;
  };
  const entities = source.entities.map(shift);
  target.entities.push(...entities);
  target.next = source.next + offset;
  return entities.map((e) => e.id);
}

/** Import geometry as independent entities, retaining layers and shared endpoints. */
export const appendDrawing = (target: Sketch, drawing: SourceDrawing): number[] => appendSketch(target, fromDrawing(drawing));
