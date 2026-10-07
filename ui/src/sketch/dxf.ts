// The sketch as an ASCII DXF (R12 entities, millimetres) for the part
// importer and for other CAD programs. Construction geometry is left out.
import { angleOf, dist } from './geometry';
import { point, type Sketch } from './model';

const num = (v: number): string => {
  const s = (Math.abs(v) < 1e-12 ? 0 : v).toFixed(6).replace(/\.?0+$/, '');
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
    } else {
      const c = point(sketch, e.c), a = point(sketch, e.a), b = point(sketch, e.b);
      out.push('0', 'ARC', '8', layer, '10', num(c.x), '20', num(c.y), '30', '0', '40', num(dist(c, a)), '50', deg(angleOf(c, a)), '51', deg(angleOf(c, b)));
    }
  }
  out.push('0', 'ENDSEC', '0', 'EOF');
  return out.join('\r\n') + '\r\n';
}

/** Whether the sketch has anything to cut. */
export const hasCut = (sketch: Sketch): boolean => sketch.entities.some((e) => !e.construction);
