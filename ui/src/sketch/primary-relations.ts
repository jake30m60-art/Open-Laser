import { addConstraint, clone, point, type Sketch } from './model';
/** Essential axes of straight edges, without links to neighbouring objects. */
export function primaryRelations(source: Sketch): Sketch {
 const sketch=clone(source);
 for(const edge of sketch.entities){
  if(edge.kind!=='line'||edge.construction||sketch.constraints.some(c=>(c.kind==='horizontal'||c.kind==='vertical')&&c.line===edge.id))continue;
  const a=point(sketch,edge.a),b=point(sketch,edge.b);
  if(Math.hypot(a.x-b.x,a.y-b.y)<1e-8)continue;
  if(Math.abs(a.y-b.y)<1e-8)addConstraint(sketch,{kind:'horizontal',line:edge.id});
  else if(Math.abs(a.x-b.x)<1e-8)addConstraint(sketch,{kind:'vertical',line:edge.id});
 }
 return sketch;
}
