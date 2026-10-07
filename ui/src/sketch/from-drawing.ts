import type { Curve } from '../api';
import { emptySketch, addPoint, addEntity, type Sketch } from './model';
export type SourceDrawing = { contours: Array<{ layer: string; curves: Curve[] }> };
/** Import exact source lines/arcs into the editor, retaining DXF layers. */
export function fromDrawing(drawing: SourceDrawing): Sketch {
  const sketch = emptySketch();
  const points = new Map<string, number>();
  const endpoint = (x: number, y: number) => {
    const key = x.toFixed(8)+','+y.toFixed(8);
    let id = points.get(key); if (id === undefined) { id = addPoint(sketch,x,y); points.set(key,id); } return id;
  };
  for (const contour of drawing.contours) {
    points.clear();
    for (const curve of contour.curves) {
    const layer = contour.layer;
    if (curve.type === 'line') addEntity(sketch,{kind:'line',a:endpoint(curve.start.x,curve.start.y),b:endpoint(curve.end.x,curve.end.y),layer});
    else {
      const c = addPoint(sketch,curve.center.x,curve.center.y);
      if (Math.abs(curve.sweep) >= 2*Math.PI-1e-8) addEntity(sketch,{kind:'circle',c,r:curve.radius,layer});
      else {
        const start=curve.sweep<0 ? curve.start_angle+curve.sweep : curve.start_angle;
        const end=curve.sweep<0 ? curve.start_angle : curve.start_angle+curve.sweep;
        addEntity(sketch,{kind:'arc',c,a:endpoint(curve.center.x+curve.radius*Math.cos(start),curve.center.y+curve.radius*Math.sin(start)),b:endpoint(curve.center.x+curve.radius*Math.cos(end),curve.center.y+curve.radius*Math.sin(end)),layer});
      }
    }
  }
  }
  return sketch;
}
