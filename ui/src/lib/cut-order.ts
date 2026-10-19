import type { Preview } from '../api';

type Point = [number, number];
export type PreviewSegment = { a: Point; b: Point; contour: number; length: number; kind: string };

/** Display geometry only; this animation does not estimate machine timing. */
export function simulationSegments(preview: Preview | null, marked: ReadonlySet<string>, mode: 'all' | 'cut' | 'mark', excluded: ReadonlySet<number> = new Set()): PreviewSegment[] {
  return orderSegments(preview, excluded).filter(s => mode === 'all' || marked.has(preview!.contours[s.contour]!.layer) === (mode === 'mark'));
}

export function orderSegments(preview: Preview | null, excluded: ReadonlySet<number> = new Set()): PreviewSegment[] {
  return (preview?.contours ?? []).flatMap((c, contour) => c.sources.some(s => excluded.has(s)) ? [] : c.paths.flatMap(p => p.points.slice(1).map((point, i) => {
    const a = p.points[i]! as Point, b = point as Point;
    return { a, b, contour, length: Math.hypot(b[0] - a[0], b[1] - a[1]), kind: p.kind };
  })));
}

export function orderPosition(segments: PreviewSegment[], progress: number): { point: Point; contour: number } | null {
  let left = Math.max(0, Math.min(1, progress)) * segments.reduce((sum, s) => sum + s.length, 0);
  for (const s of segments) {
    if (left <= s.length && s.length > 0) {
      const t = left / s.length;
      return { point: [s.a[0] + (s.b[0] - s.a[0]) * t, s.a[1] + (s.b[1] - s.a[1]) * t], contour: s.contour };
    }
    left -= s.length;
  }
  const last = segments.at(-1);
  return last ? { point: last.b, contour: last.contour } : null;
}

/** What the head has run up to `progress`, as Mlaser colours it: the cut
 *  stretches and the moves with the laser off, each as one SVG path. */
export function orderTrail(segments: PreviewSegment[], progress: number, marked: ReadonlySet<number> = new Set()): { cut: string; mark: string; moved: string } {
  let left = Math.max(0, Math.min(1, progress)) * segments.reduce((sum, s) => sum + s.length, 0);
  const cut: string[] = [], mark: string[] = [], moved: string[] = [];
  for (const s of segments) {
    if (left <= 0) break;
    const t = s.length > 0 ? Math.min(1, left / s.length) : 1;
    const end: Point = [s.a[0] + (s.b[0] - s.a[0]) * t, s.a[1] + (s.b[1] - s.a[1]) * t];
    (s.kind === 'travel' ? moved : marked.has(s.contour) ? mark : cut).push(`M${s.a[0]} ${s.a[1]}L${end[0]} ${end[1]}`);
    left -= s.length;
  }
  return { cut: cut.join(''), mark: mark.join(''), moved: moved.join('') };
}

function phases(segments: readonly PreviewSegment[], layer: (contour: number) => string): Array<{start:number;length:number}> {
  const ranges: Array<{start:number;length:number}> = []; let walked=0, previous: string | null=null;
  for(const segment of segments) { const name=layer(segment.contour); if(name!==previous) ranges.push({start:walked,length:0}); ranges.at(-1)!.length+=segment.length; walked+=segment.length; previous=name; }
  return ranges;
}
/** Equal time per consecutive layer, preserving the actual path order within it. */
export function simulationDistanceProgress(segments: readonly PreviewSegment[], progress: number, layer: (contour:number)=>string): number {
  const ranges=phases(segments,layer), total=segments.reduce((sum,s)=>sum+s.length,0);
  if(!ranges.length || !total) return 0;
  const p=Math.max(0,Math.min(1,progress))*ranges.length, index=Math.min(ranges.length-1,Math.floor(p)), range=ranges[index]!;
  return (range.start+range.length*(p-index))/total;
}
export function simulationTimeProgress(segments: readonly PreviewSegment[], distanceProgress: number, layer: (contour:number)=>string): number {
  const ranges=phases(segments,layer), total=segments.reduce((sum,s)=>sum+s.length,0), walked=Math.max(0,Math.min(1,distanceProgress))*total;
  const index=ranges.findIndex(r=>walked<=r.start+r.length);
  if(index<0 || !ranges.length) return 1;
  const range=ranges[index]!;return (index+(range.length ? (walked-range.start)/range.length : 0))/ranges.length;
}

/** Adjacent scan chunks of one contour are one operator-visible operation. */
export function simulationSteps(preview: Preview | null, segments: readonly PreviewSegment[]): Array<{layer:string;sources:number[];contours:number[];distance:number}> {
  const steps: Array<{layer:string;sources:number[];contours:number[];distance:number}> = [];
  let walked=0, previous='';
  for(const segment of segments) {
    const contour=preview?.contours[segment.contour]; if(!contour) continue;
    const key=JSON.stringify([contour.layer,contour.sources]);
    if(key!==previous) steps.push({layer:contour.layer,sources:contour.sources,contours:[],distance:walked});
    const step=steps.at(-1)!;if(!step.contours.includes(segment.contour)) step.contours.push(segment.contour);
    walked+=segment.length;previous=key;
  }
  return steps;
}

/** Machine time along the segments: each segment's length over its speed. */
export function segmentSeconds(segments: readonly PreviewSegment[], speed: (segment: PreviewSegment) => number): number[] {
  return segments.map(s => s.length / Math.max(0.1, speed(s)));
}
/** Distance progress reached after `timeProgress` of the machine time. */
export function distanceAtTime(segments: readonly PreviewSegment[], seconds: readonly number[], timeProgress: number): number {
  const total = seconds.reduce((a, b) => a + b, 0), length = segments.reduce((a, s) => a + s.length, 0);
  if (!total || !length) return 0;
  let left = Math.max(0, Math.min(1, timeProgress)) * total, walked = 0;
  for (let i = 0; i < segments.length; i++) {
    const t = seconds[i]!, l = segments[i]!.length;
    if (left <= t) return (walked + (t ? l * left / t : 0)) / length;
    left -= t; walked += l;
  }
  return 1;
}
/** Machine-time progress at `distanceProgress` along the segments. */
export function timeAtDistance(segments: readonly PreviewSegment[], seconds: readonly number[], distanceProgress: number): number {
  const total = seconds.reduce((a, b) => a + b, 0), length = segments.reduce((a, s) => a + s.length, 0);
  if (!total || !length) return 0;
  let left = Math.max(0, Math.min(1, distanceProgress)) * length, spent = 0;
  for (let i = 0; i < segments.length; i++) {
    const t = seconds[i]!, l = segments[i]!.length;
    if (left <= l) return (spent + (l ? t * left / l : 0)) / total;
    left -= l; spent += t;
  }
  return 1;
}
