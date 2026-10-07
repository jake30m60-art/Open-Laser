import type { PreviewContour } from '../api';
import { pathOf } from './svg';
type Point = [number, number];
/** Width preview in drawing coordinates, before the second endpoint snaps. */
export function channelBand(first: Point, second: Point, width: number): string {
  const dx = second[0]-first[0], dy = second[1]-first[1], length = Math.hypot(dx,dy);
  if (!length || !Number.isFinite(width) || width <= 0) return '';
  const half = width/2, nx = -dy/length*half, ny = dx/length*half;
  return pathOf([[first[0]+nx,first[1]+ny],[second[0]+nx,second[1]+ny],[second[0]-nx,second[1]-ny],[first[0]-nx,first[1]-ny]], false)+'Z';
}

/** Project onto a visible cut edge for the hover preview; no server mutation. */
export function snapBridge(contours: readonly PreviewContour[], at: Point, tolerance: number): {point:Point;direction:Point} | null {
 let best: {point:Point;direction:Point} | null=null, distance=tolerance;
 for(const c of contours)for(const p of c.paths)if(p.kind==='cut')for(let i=1;i<p.points.length;i++){
  const a=p.points[i-1]!,b=p.points[i]!,dx=b[0]!-a[0]!,dy=b[1]!-a[1]!,length=Math.hypot(dx,dy);
  if(!length)continue;
  const t=Math.max(0,Math.min(1,((at[0]-a[0]!)*dx+(at[1]-a[1]!)*dy)/(length*length)));
  const point:Point=[a[0]!+dx*t,a[1]!+dy*t],gap=Math.hypot(point[0]-at[0],point[1]-at[1]);
  if(gap<=distance){distance=gap;best={point,direction:[dx/length,dy/length]};}
 }
 return best;
}
export function bridgeOpening(contours: readonly PreviewContour[], at: Point, width:number): string {
 const snap=snapBridge(contours,at,0.1);if(!snap || width<=0)return '';
 const [x,y]=snap.point,[dx,dy]=snap.direction,h=width/2;
 return pathOf([[x-dx*h,y-dy*h],[x+dx*h,y+dy*h]],false);
}
