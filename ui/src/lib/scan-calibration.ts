// Geometry shared by the calibration preview and its placement checks.
export type XY = readonly [number, number];
export const foldedScanAngle = (angle: number): number => {
  const a = ((angle % 180) + 180) % 180;
  return 180 - a < 0.05 ? 0 : a;
};
/** Jerk-limited ramp plus 10% and 5 mm reserve, matching the server. */
/** Seconds at full speed, laser off, before a line marks, as the server. */
export const SCAN_SETTLE_SECONDS = 0.05;
export const scanRunUp = (speed: number, acceleration = 6000, time = 0.25): number => 0.55 * speed * (speed <= acceleration * time * 0.5 ? time : speed / acceleration + time * 0.5) + speed * SCAN_SETTLE_SECONDS + 5;
export function scanPadding(angle: number, speed: number, acceleration = 6000, time = 0.25): [number, number] {
  const a = foldedScanAngle(angle) * Math.PI / 180, distance = scanRunUp(speed, acceleration, time);
  return [distance * Math.abs(Math.cos(a)), distance * Math.abs(Math.sin(a))];
}
export function scanEnvelope(corner: XY, size: XY, angle: number, speed: number, acceleration = 6000, time = 0.25) {
  const [x, y] = scanPadding(angle, speed, acceleration, time);
  return { min: { x: corner[0] - x, y: corner[1] - y }, max: { x: corner[0] + size[0] + x, y: corner[1] + size[1] + y } };
}
export function envelopeFits(envelope: ReturnType<typeof scanEnvelope>, bed: { min: { x: number; y: number }; max: { x: number; y: number } }): boolean {
  return envelope.min.x >= bed.min.x - 1e-9 && envelope.min.y >= bed.min.y - 1e-9
    && envelope.max.x <= bed.max.x + 1e-9 && envelope.max.y <= bed.max.y + 1e-9;
}

export function scanGroupSize(length: number, spacing: number, angle: number): [number, number] {
  const a = foldedScanAngle(angle) * Math.PI / 180;
  const c = Math.abs(Math.cos(a)), s = Math.abs(Math.sin(a));
  return [length * c + 4 * spacing * s, length * s + 4 * spacing * c];
}
/** Mirrors the server: speed first, then direction, four fields per row. */
export function scanTestLayout(origin: XY, speeds: number[], angles: number[], length: number, spacing: number, acceleration = 6000, time = 0.25) {
  const tests = speeds.flatMap(speed => angles.map(angle => ({ speed, angle: foldedScanAngle(angle), size: scanGroupSize(length, spacing, angle) })));
  const padding = [0, 1].map(axis => Math.max(...tests.map(test => scanPadding(test.angle, test.speed, acceleration, time)[axis]!))) as [number, number];
  const cell = [0, 1].map(axis => Math.max(...tests.map(test => test.size[axis]!))) as [number, number];
  const columns = angles.length > 1 ? angles.length : 4;
  const placed = tests.map((test, i) => ({ ...test, corner: [origin[0] + (i % columns) * (cell[0] + 10), origin[1] + Math.floor(i / columns) * (cell[1] + 10)] as [number, number] }));
  const before = [0, 1].map(axis => Math.max(0, ...placed.map(t => scanPadding(t.angle, t.speed, acceleration, time)[axis]! - (t.corner[axis]! - origin[axis]!)))) as [number, number];
  const markedSize = [0, 1].map(axis => Math.max(...placed.map(t => t.corner[axis]! + t.size[axis]! - origin[axis]!))) as [number, number];
  const after = [0, 1].map(axis => Math.max(0, ...placed.map(t => t.corner[axis]! - origin[axis]! + t.size[axis]! + scanPadding(t.angle, t.speed, acceleration, time)[axis]! - markedSize[axis]!))) as [number, number];
  return { padding, before, after, tests: placed };
}
