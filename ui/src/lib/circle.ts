// The circle through three points, as the server's disc measurement finds it.

export type Point = [number, number];

/** Centre and diameter of the circle through three points, or null when they lie on a line. */
export function circleThrough([p1, p2, p3]: [Point, Point, Point]): { center: Point; diameter: number } | null {
  const det = 2 * (p1[0] * (p2[1] - p3[1]) + p2[0] * (p3[1] - p1[1]) + p3[0] * (p1[1] - p2[1]));
  if (Math.abs(det) < 1e-9) return null;
  const sq = (p: Point) => p[0] * p[0] + p[1] * p[1];
  const x = (sq(p1) * (p2[1] - p3[1]) + sq(p2) * (p3[1] - p1[1]) + sq(p3) * (p1[1] - p2[1])) / det;
  const y = (sq(p1) * (p3[0] - p2[0]) + sq(p2) * (p1[0] - p3[0]) + sq(p3) * (p2[0] - p1[0])) / det;
  return { center: [x, y], diameter: 2 * Math.hypot(p1[0] - x, p1[1] - y) };
}
