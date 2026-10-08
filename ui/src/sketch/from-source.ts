// A DXF file's curves as the sketch keeps them: what the server read from
// the part's original file, ellipses and splines as they were drawn.
import { addEntity, addPoint, emptySketch, type Sketch } from './model';

type XY = [number, number];
export type SourceCurve =
  | { layer: string; kind: 'line'; start: XY; end: XY }
  | { layer: string; kind: 'arc'; center: XY; radius: number; start_angle: number; sweep: number }
  | { layer: string; kind: 'ellipse'; center: XY; major: XY; minor_radius: number; start: number; end: number }
  | { layer: string; kind: 'spline'; degree: number; knots: number[]; poles: XY[]; weights: number[] };

/** The sketch of the curves, ends that meet sharing one point. */
export function fromSource(curves: readonly SourceCurve[]): Sketch {
  const sketch = emptySketch();
  const ends = new Map<string, number>();
  // Ends a micrometre apart are one point: the file drew them to meet.
  const endpoint = (x: number, y: number) => {
    const key = `${Math.round(x * 1000)},${Math.round(y * 1000)}`;
    let id = ends.get(key);
    if (id === undefined) { id = addPoint(sketch, x, y); ends.set(key, id); }
    return id;
  };
  for (const curve of curves) {
    const layer = curve.layer;
    if (curve.kind === 'line') {
      addEntity(sketch, { kind: 'line', a: endpoint(...curve.start), b: endpoint(...curve.end), layer });
    } else if (curve.kind === 'arc') {
      const [cx, cy] = curve.center, r = curve.radius;
      const c = addPoint(sketch, cx, cy);
      if (Math.abs(curve.sweep) >= 2 * Math.PI - 1e-8) { addEntity(sketch, { kind: 'circle', c, r, layer }); continue; }
      const from = curve.sweep < 0 ? curve.start_angle + curve.sweep : curve.start_angle;
      const to = curve.sweep < 0 ? curve.start_angle : curve.start_angle + curve.sweep;
      addEntity(sketch, { kind: 'arc', c, a: endpoint(cx + r * Math.cos(from), cy + r * Math.sin(from)), b: endpoint(cx + r * Math.cos(to), cy + r * Math.sin(to)), layer });
    } else if (curve.kind === 'ellipse') {
      const [cx, cy] = curve.center, [ux, uy] = curve.major;
      const a = Math.hypot(ux, uy) || 1, k = curve.minor_radius / a;
      const at = (t: number): XY => [cx + ux * Math.cos(t) - uy * k * Math.sin(t), cy + uy * Math.cos(t) + ux * k * Math.sin(t)];
      const c = addPoint(sketch, cx, cy), m = addPoint(sketch, cx + ux, cy + uy);
      if (curve.end - curve.start >= 2 * Math.PI - 1e-9) addEntity(sketch, { kind: 'ellipse', c, m, r: curve.minor_radius, layer });
      else addEntity(sketch, { kind: 'ellipse-arc', c, m, r: curve.minor_radius, a: endpoint(...at(curve.start)), b: endpoint(...at(curve.end)), layer });
    } else {
      const last = curve.poles.length - 1;
      const poles = curve.poles.map(([x, y], i) => (i === 0 || i === last ? endpoint(x, y) : addPoint(sketch, x, y)));
      const rational = curve.weights.some((w) => Math.abs(w - 1) > 1e-12);
      addEntity(sketch, { kind: 'spline', poles, degree: curve.degree, knots: [...curve.knots], ...(rational ? { weights: [...curve.weights] } : {}), layer });
    }
  }
  return sketch;
}
