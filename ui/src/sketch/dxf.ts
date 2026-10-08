// The sketch as an ASCII DXF (R12 entities, millimetres) for the part
// importer and for other CAD programs. Construction geometry is left out.
import { angleOf, dist } from './geometry';
import { placedText, point, type Sketch } from './model';
import { arcParams, ellipseFrame } from './curves';

const num = (v: number): string => {
  const s = (Math.abs(v) < 1e-12 ? 0 : v).toFixed(6).replace(/\.?0+$/, '');
  return s === '-0' ? '0' : s;
};
/** Angles in radians and ratios, exact enough that curve ends land on their neighbours. */
const fine = (v: number): string => {
  const s = (Math.abs(v) < 1e-15 ? 0 : v).toFixed(12).replace(/\.?0+$/, '');
  return s === '-0' ? '0' : s;
};
const deg = (radians: number): string => num((((radians * 180) / Math.PI) % 360 + 360) % 360);

export function toDxf(sketch: Sketch): string {
  const out: string[] = ['0', 'SECTION', '2', 'HEADER', '9', '$ACADVER', '1', 'AC1009', '9', '$INSUNITS', '70', '4', '0', 'ENDSEC', '0', 'SECTION', '2', 'ENTITIES'];
  for (const e of sketch.entities) {
    if (e.construction) continue;
    const layer = (e.layer ?? '0').replace(/[\r\n]/g,' ');
    if (e.kind === 'line') {
      const a = point(sketch, e.a), b = point(sketch, e.b);
      out.push('0', 'LINE', '8', layer, '10', num(a.x), '20', num(a.y), '30', '0', '11', num(b.x), '21', num(b.y), '31', '0');
    } else if (e.kind === 'circle') {
      const c = point(sketch, e.c);
      out.push('0', 'CIRCLE', '8', layer, '10', num(c.x), '20', num(c.y), '30', '0', '40', num(e.r));
    } else if (e.kind === 'ellipse' || e.kind === 'ellipse-arc') {
      // DXF wants the longer axis as the major one; a wide minor axis swaps them.
      const { c, u, v } = ellipseFrame(sketch, e);
      const [from, to] = e.kind === 'ellipse' ? [0, 2 * Math.PI] : arcParams(sketch, e);
      const a = Math.hypot(u.x, u.y), b = Math.hypot(v.x, v.y);
      const [major, ratio, shift] = b > a ? [v, a / b, -Math.PI / 2] : [u, b / a, 0];
      out.push('0', 'ELLIPSE', '8', layer, '10', num(c.x), '20', num(c.y), '30', '0', '11', num(major.x), '21', num(major.y), '31', '0',
        '210', '0', '220', '0', '230', '1', '40', fine(ratio), '41', fine(from + shift), '42', fine(to + shift));
    } else if (e.kind === 'spline') {
      const rational = !!e.weights?.some((w) => Math.abs(w - 1) > 1e-12);
      out.push('0', 'SPLINE', '8', layer, '210', '0', '220', '0', '230', '1', '70', String(8 + (rational ? 4 : 0)), '71', String(e.degree),
        '72', String(e.knots.length), '73', String(e.poles.length), '74', '0');
      for (const k of e.knots) out.push('40', fine(k));
      e.poles.forEach((id, i) => {
        const p = point(sketch, id);
        out.push('10', num(p.x), '20', num(p.y), '30', '0');
        if (rational) out.push('41', fine(e.weights![i] ?? 1));
      });
    } else {
      const c = point(sketch, e.c), a = point(sketch, e.a), b = point(sketch, e.b);
      out.push('0', 'ARC', '8', layer, '10', num(c.x), '20', num(c.y), '30', '0', '40', num(dist(c, a)), '50', deg(angleOf(c, a)), '51', deg(angleOf(c, b)));
    }
  }
  // Texts as their closed outlines.
  for (const text of sketch.texts ?? []) {
    for (const line of placedText(text)) {
      const closed = line.length > 2 && Math.hypot(line[0]![0] - line.at(-1)![0], line[0]![1] - line.at(-1)![1]) < 1e-6;
      const pts = closed ? line.slice(0, -1) : line;
      out.push('0', 'LWPOLYLINE', '8', 'Text', '90', String(pts.length), '70', closed ? '1' : '0');
      for (const [x, y] of pts) out.push('10', num(x), '20', num(y));
    }
  }
  out.push('0', 'ENDSEC', '0', 'EOF');
  return out.join('\r\n') + '\r\n';
}

/** Whether the sketch has anything to cut. */
export const hasCut = (sketch: Sketch): boolean => sketch.entities.some((e) => !e.construction) || !!sketch.texts?.length;
