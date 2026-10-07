import { fromDrawing, type SourceDrawing } from './from-drawing';import type { Sketch } from './model';
/** Import geometry as independent entities, retaining layers and shared endpoints. */
export function appendDrawing(target:Sketch,drawing:SourceDrawing):number[]{
 const source=fromDrawing(drawing),offset=target.next-1;
 target.points.push(...source.points.map(p=>({...p,id:p.id+offset})));
 const entities=source.entities.map(e=>e.kind==='line'?{...e,id:e.id+offset,a:e.a+offset,b:e.b+offset}:e.kind==='circle'?{...e,id:e.id+offset,c:e.c+offset}:{...e,id:e.id+offset,c:e.c+offset,a:e.a+offset,b:e.b+offset});
 target.entities.push(...entities);target.next=source.next+offset;return entities.map(e=>e.id);
}
