// Where the job's rectangular sheet lies on the bed: its own origin, set at
// the head or found by the edge search, apart from the job origin; until
// then at the bed minimum.
import type { DraftView } from '../api';
import type { Box } from './svg';

/** The sheet's outline in machine millimetres, or null without a rectangular sheet. */
export function sheetOnBed(draft: DraftView | null | undefined, bed: Box | null, turned = false): [number, number][] | null {
  const stock = draft?.nesting?.stock;
  if (stock?.kind !== 'rectangle') return null;
  const width = stock.bounds.max.x - stock.bounds.min.x;
  const height = stock.bounds.max.y - stock.bounds.min.y;
  const [x, y] = draft!.sheet_corner ?? (bed ? [bed.minX, bed.minY] : [stock.bounds.min.x, stock.bounds.min.y]);
  const square: [number, number][] = [[x, y], [x + width, y], [x + width, y + height], [x, y + height]];
  // As it lies: a sheet measured at an angle turns about its corner, as the
  // machine paths do. Setup keeps it square.
  const angle = turned ? draft!.sheet_angle_deg ?? 0 : 0;
  if (!angle) return square;
  const [sin, cos] = [Math.sin(angle * Math.PI / 180), Math.cos(angle * Math.PI / 180)];
  return square.map(([px, py]) => [x + cos * (px - x) - sin * (py - y), y + sin * (px - x) + cos * (py - y)]);
}
