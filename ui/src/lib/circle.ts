// The circle through edge points, as the server's disc measurement fits it.

export type Point = [number, number];

/**
 * The circle best through three or more points, least squares in
 * x² + y² + Dx + Ey + F = 0 (through three it is exact): its centre,
 * diameter and how far the farthest point lies off it; null when the
 * points lie on a line.
 */
export function circleThrough(points: Point[]): { center: Point; diameter: number; off: number } | null {
  const n = points.length;
  const mean: Point = [points.reduce((s, p) => s + p[0], 0) / n, points.reduce((s, p) => s + p[1], 0) / n];
  const m = [[0, 0, 0], [0, 0, 0], [0, 0, 0]];
  const v = [0, 0, 0];
  for (const p of points) {
    const row = [p[0] - mean[0], p[1] - mean[1], 1];
    const rhs = -(row[0]! * row[0]! + row[1]! * row[1]!);
    for (let i = 0; i < 3; i++) {
      for (let j = 0; j < 3; j++) m[i]![j]! += row[i]! * row[j]!;
      v[i]! += row[i]! * rhs;
    }
  }
  const det3 = (a: number[][]): number =>
    a[0]![0]! * (a[1]![1]! * a[2]![2]! - a[1]![2]! * a[2]![1]!)
    - a[0]![1]! * (a[1]![0]! * a[2]![2]! - a[1]![2]! * a[2]![0]!)
    + a[0]![2]! * (a[1]![0]! * a[2]![1]! - a[1]![1]! * a[2]![0]!);
  const det = det3(m);
  if (Math.abs(det) < 1e-9) return null;
  const solve = (k: number): number => det3(m.map((row, i) => row.map((x, j) => (j === k ? v[i]! : x)))) / det;
  const c: Point = [-solve(0) / 2, -solve(1) / 2];
  const squared = c[0] * c[0] + c[1] * c[1] - solve(2);
  if (squared <= 0) return null;
  const radius = Math.sqrt(squared);
  const center: Point = [c[0] + mean[0], c[1] + mean[1]];
  const off = Math.max(...points.map((p) => Math.abs(Math.hypot(p[0] - center[0], p[1] - center[1]) - radius)));
  return { center, diameter: 2 * radius, off };
}
