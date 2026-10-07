// The arrow that shows which way a contour is cut, as Mlaser draws it: in
// the middle of its longest straight stretch, away from the corners,
// pointing the way the head runs.

/** An arrowhead `size` long on `points`, as an SVG path, or nothing for a
 *  path too short to carry one. */
export function arrowAlong(points: ReadonlyArray<readonly number[]>, size: number): string | null {
  let total = 0, best = -1, longest = 0;
  for (let i = 1; i < points.length; i++) {
    const length = Math.hypot(points[i]![0]! - points[i - 1]![0]!, points[i]![1]! - points[i - 1]![1]!);
    total += length;
    if (length > longest) { longest = length; best = i; }
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
