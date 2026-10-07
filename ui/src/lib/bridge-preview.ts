import { pathOf } from './svg';
type Point = [number, number];
/** Width preview in drawing coordinates, before the second endpoint snaps. */
export function channelBand(first: Point, second: Point, width: number): string {
  const dx = second[0]-first[0], dy = second[1]-first[1], length = Math.hypot(dx,dy);
  if (!length || !Number.isFinite(width) || width <= 0) return '';
  const half = width/2, nx = -dy/length*half, ny = dx/length*half;
  return pathOf([[first[0]+nx,first[1]+ny],[second[0]+nx,second[1]+ny],[second[0]-nx,second[1]-ny],[first[0]-nx,first[1]-ny]], false)+'Z';
}
