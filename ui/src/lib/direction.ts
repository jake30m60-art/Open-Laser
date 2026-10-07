// The arrow that shows which way a contour is cut, as Mlaser draws it: in
// the middle of its longest straight stretch (the topmost of equals), away from the corners,
// pointing the way the head runs.

/** An arrowhead `size` long on `points`, as an SVG path, or nothing for a
 *  path too short to carry one. */
export function arrowAlong(points: ReadonlyArray<readonly number[]>, size: number): string | null {
  let total = 0, longest = 0;
  const lengths: number[] = [0];
  for (let i = 1; i < points.length; i++) {
    const length = Math.hypot(points[i]![0]! - points[i - 1]![0]!, points[i]![1]! - points[i - 1]![1]!);
    lengths.push(length);
    total += length;
    longest = Math.max(longest, length);
  }
  // Of stretches alike in length, the topmost, then leftmost: the same
  // place whichever way the contour runs, so a reversed cut turns the arrow
  // round rather than moving it to another side.
  let best = -1, bestY = -Infinity, bestX = Infinity;
  for (let i = 1; i < points.length; i++) {
    if (lengths[i]! < longest * (1 - 1e-6)) continue;
    const y = (points[i]![1]! + points[i - 1]![1]!) / 2, x = (points[i]![0]! + points[i - 1]![0]!) / 2;
    if (y > bestY + 1e-9 || (Math.abs(y - bestY) <= 1e-9 && x < bestX)) { best = i; bestY = y; bestX = x; }
  }
  if (!(total > size * 3) || best < 0) return null;
  const [ax, ay] = points[best - 1] as [number, number];
  const [bx, by] = points[best] as [number, number];
  const ux = (bx - ax) / longest, uy = (by - ay) / longest;
  const tipX = (ax + bx) / 2 + ux * size / 2, tipY = (ay + by) / 2 + uy * size / 2;
  const baseX = tipX - ux * size, baseY = tipY - uy * size;
  const half = size * 0.45;
  const f = (n: number) => Number(n.toFixed(4));
  return `M${f(tipX)} ${f(tipY)}L${f(baseX - uy * half)} ${f(baseY + ux * half)}L${f(baseX + uy * half)} ${f(baseY - ux * half)}Z`;
}
