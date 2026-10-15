// Where the job's rectangular sheet lies on the bed: its own origin, set at
// the head or found by the edge search, apart from the job origin; until
// then at the bed minimum.
import type { DraftView } from '../api';
import type { Box } from './svg';

/** The sheet's outline in machine millimetres, or null without a rectangular sheet. */
export function sheetOnBed(draft: DraftView | null | undefined, bed: Box | null): [number, number][] | null {
  const stock = draft?.nesting?.stock;
  if (stock?.kind !== 'rectangle') return null;
  const width = stock.bounds.max.x - stock.bounds.min.x;
  const height = stock.bounds.max.y - stock.bounds.min.y;
  const [x, y] = draft!.sheet_corner ?? (bed ? [bed.minX, bed.minY] : [stock.bounds.min.x, stock.bounds.min.y]);
  return [[x, y], [x + width, y], [x + width, y + height], [x, y + height]];
}
