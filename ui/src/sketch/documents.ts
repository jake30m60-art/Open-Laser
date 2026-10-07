import { isSketch, type Sketch } from './model';
export type SketchTarget='new'|'add'|'replace';
export type SketchDocument={id:string;name:string;sketch:Sketch;target:SketchTarget;sourcePart?:string;jobKey?:string};
export function readDocuments(value:string|null): SketchDocument[] {
 try { const entries=JSON.parse(value ?? '[]'); if(!Array.isArray(entries))return [];
 return entries.filter((d:Partial<SketchDocument>)=>d && typeof d.id==='string' && typeof d.name==='string' && isSketch(d.sketch) && ['new','add','replace'].includes(d.target ?? '')); }catch{return [];}
}
