import type { Preview } from '../api';
export type OrderRow = { sources: number[]; layer: string; depth: number; closed: boolean };
/** One row per physical contour; repeated scan chunks remain one operation. */
export function orderRows(preview: Preview | null): OrderRow[] {
  const seen = new Set<number>(); const rows: OrderRow[] = [];
  for (const c of preview?.contours ?? []) {
    const sources = c.sources.filter(s => !seen.has(s));
    if (!sources.length) continue;
    sources.forEach(s => seen.add(s));
    rows.push({ sources, layer: c.layer, depth: c.depth, closed: c.closed });
  }
  return rows;
}
/** Move a complete bridged contour, preserving the exact source permutation. */
export function moveOrderRow(rows: OrderRow[], from: number, to: number): number[] | null {
  if (from < 0 || to < 0 || from >= rows.length || to >= rows.length || rows[from]!.layer !== rows[to]!.layer) return null;
  const copy = [...rows]; const [row] = copy.splice(from, 1); copy.splice(to, 0, row!);
  return copy.flatMap(r => r.sources);
}
