import type { Curve, Compiled } from '../api';
import type { SourceDrawing } from '../sketch/from-drawing';
export type XY = [number, number];
export type Shape = { kind: 'rectangle'; width: number; height: number } | { kind: 'disc'; diameter: number } | { kind: 'special'; drawing: SourceDrawing };
export type Pick = { contour: number; curve: number; t: number };
export type Template = { remnant?: {id: string; revision: number} | null; shape: Shape; picks: Pick[]; drawing_point: XY; machine_point: XY; search_mm: number; angle_deg: number; tolerance_mm: number; every_job: boolean };
export type Pose = { translation: XY; angle: number };
export type Search = { pick: Pick; point: XY; start: XY; end: XY; normal: XY };
export type Plan = { searches: Search[]; outline: XY[][]; nominal: Pose };
export type Fit = { pose: Pose; max_error_mm: number; hits: XY[] };
export type AlignmentView = { revision: number; enabled: boolean; template: Template | null; phase: 'idle'|'measuring'|'ready'|'running'|'paused'|'failed'|'stale'; result: Fit | null; hardware_available: boolean; lid_verified: boolean };
export type SimulationReply = { view: AlignmentView; plan: Plan; compiled: Compiled | null; compile_problem: string | null };
export function defaultTemplate(): Template {
  return { shape: { kind: 'rectangle', width: 600, height: 500 }, picks: [], drawing_point: [300,250], machine_point: [700,600], search_mm: 50, angle_deg: 5, tolerance_mm: 0.1, every_job: true };
}
export function referenceDrawing(shape: Shape): SourceDrawing {
  if (shape.kind === 'special') return shape.drawing;
  if (shape.kind === 'disc') return { contours: [{ layer: 'reference', curves: [{ type: 'arc', center: { x: 0, y: 0 }, radius: shape.diameter/2, start_angle: 0, sweep: 2*Math.PI }] }] };
  const p: XY[] = [[0,0],[shape.width,0],[shape.width,shape.height],[0,shape.height]];
  return { contours: [{ layer: 'reference', curves: p.map((a,i) => ({ type: 'line', start: {x:a[0],y:a[1]}, end: {x:p[(i+1)%4]![0],y:p[(i+1)%4]![1]} })) }] };
}
export function pointOn(curve: Curve, t: number): XY {
  return curve.type === 'line' ? [curve.start.x+(curve.end.x-curve.start.x)*t,curve.start.y+(curve.end.y-curve.start.y)*t]
    : [curve.center.x+curve.radius*Math.cos(curve.start_angle+curve.sweep*t),curve.center.y+curve.radius*Math.sin(curve.start_angle+curve.sweep*t)];
}
export function curvePoints(curve: Curve): XY[] {return Array.from({length:curve.type==='line'?2:97},(_,i)=>pointOn(curve,i/(curve.type==='line'?1:96)));}
export function nearestPick(drawing: SourceDrawing, p: XY): {pick: Pick; distance: number} | null {
  let best: {pick: Pick; distance: number} | null = null;
  drawing.contours.forEach((c,contour)=>c.curves.forEach((curve,index)=>{
    let t: number;
    if (curve.type==='line') {
      const dx=curve.end.x-curve.start.x,dy=curve.end.y-curve.start.y;
      t=((p[0]-curve.start.x)*dx+(p[1]-curve.start.y)*dy)/(dx*dx+dy*dy);
    } else {
      const a=Math.atan2(p[1]-curve.center.y,p[0]-curve.center.x);
      const wrap=(v:number)=>(v%(2*Math.PI)+2*Math.PI)%(2*Math.PI);
      t=curve.sweep>0?wrap(a-curve.start_angle)/curve.sweep:wrap(curve.start_angle-a)/-curve.sweep;
      if(t>1) {const a=pointOn(curve,0),b=pointOn(curve,1);t=Math.hypot(p[0]-a[0],p[1]-a[1])<Math.hypot(p[0]-b[0],p[1]-b[1])?0:1;}
    }
    t=Math.max(0,Math.min(1,t));const q=pointOn(curve,t);const distance=Math.hypot(p[0]-q[0],p[1]-q[1]);
    if(Number.isFinite(distance)&&(!best||distance<best.distance))best={pick:{contour,curve:index,t},distance};
  }));
  return best;
}
export function mapped(p: XY, pose: Pose): XY {const c=Math.cos(pose.angle),s=Math.sin(pose.angle);return [c*p[0]-s*p[1]+pose.translation[0],s*p[0]+c*p[1]+pose.translation[1]];}

/** Put the enclosing contour first for a physical reference; the server validates topology. */
export function outerFirst(d: SourceDrawing): SourceDrawing {
  const area=(c: SourceDrawing['contours'][number])=>{const p=c.curves.flatMap(curvePoints);return Math.abs(p.reduce((s,a,i)=>{const b=p[(i+1)%p.length]!;return s+a[0]*b[1]-a[1]*b[0];},0));};
  return {contours:[...d.contours].sort((a,b)=>area(b)-area(a))};
}
