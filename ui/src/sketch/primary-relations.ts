import { addConstraint, addEntity, addPoint, clone, point, type Id, type Sketch } from './model';
/** Essential axes of straight edges, without links to neighbouring objects,
 *  and the circle a regular polygon's corners lie on. */
export function primaryRelations(source: Sketch): Sketch {
 const sketch=clone(source);
 circumscribePolygons(sketch);
 // Sides of a polygon on a circle: one axis fixes its turn; more repeat it.
 const circleOf=(p:Id)=>sketch.constraints.find(c=>c.kind==='on'&&c.point===p&&sketch.entities.some(e=>e.id===c.curve&&e.kind==='circle'&&e.construction))?.curve;
 const polygonOf=(edge:Extract<Sketch['entities'][number],{kind:'line'}>)=>{const a=circleOf(edge.a);return a!==undefined&&a===circleOf(edge.b)?a:undefined;};
 const axisOn=new Set<Id>();
 for(const c of sketch.constraints) if(c.kind==='horizontal'||c.kind==='vertical'){const e=sketch.entities.find(x=>x.id===c.line);const p=e?.kind==='line'?polygonOf(e):undefined;if(p!==undefined)axisOn.add(p);}
 for(const edge of sketch.entities){
  if(edge.kind!=='line'||edge.construction||sketch.constraints.some(c=>(c.kind==='horizontal'||c.kind==='vertical')&&c.line===edge.id))continue;
  const polygon=polygonOf(edge);
  if(polygon!==undefined){ if(axisOn.has(polygon))continue; axisOn.add(polygon); }
  const a=point(sketch,edge.a),b=point(sketch,edge.b);
  if(Math.hypot(a.x-b.x,a.y-b.y)<1e-8)continue;
  if(Math.abs(a.y-b.y)<1e-8)addConstraint(sketch,{kind:'horizontal',line:edge.id});
  else if(Math.abs(a.x-b.x)<1e-8)addConstraint(sketch,{kind:'vertical',line:edge.id});
  else if(polygon!==undefined)axisOn.delete(polygon);
 }
 return sketch;
}

/** A closed loop of lines all kept equal, as older builds drew a polygon,
 *  gets the construction circle its corners lie on. Without it the equal
 *  sides alone let the polygon fold over when a side is dimensioned. */
function circumscribePolygons(sketch: Sketch): void {
 const lines=sketch.entities.filter((e):e is Extract<Sketch['entities'][number],{kind:'line'}>=>e.kind==='line'&&!e.construction);
 const equal=new Map<Id,Set<Id>>();
 for(const c of sketch.constraints) if(c.kind==='equal'){
  for(const [x,y] of [[c.a,c.b],[c.b,c.a]] as const){ if(!equal.has(x))equal.set(x,new Set()); equal.get(x)!.add(y); }
 }
 const done=new Set<Id>();
 for(const first of lines){
  if(done.has(first.id)||!equal.has(first.id))continue;
  // Walk the loop from this line through shared corners.
  const loop=[first]; const corners:Id[]=[first.a]; let corner=first.b;
  while(loop.length<=64){
   const nextLine=lines.find(l=>!loop.includes(l)&&(l.a===corner||l.b===corner));
   if(!nextLine)break;
   corners.push(corner);
   loop.push(nextLine); corner=nextLine.a===corner?nextLine.b:nextLine.a;
   if(corner===first.a)break;
  }
  if(corner!==first.a||loop.length<3)continue;
  for(const l of loop)done.add(l.id);
  const linked=new Set<Id>([first.id]); const queue=[first.id];
  while(queue.length){ const id=queue.pop()!; for(const o of equal.get(id)??[]) if(!linked.has(o)){linked.add(o);queue.push(o);} }
  if(!loop.every(l=>linked.has(l.id)))continue;
  if(sketch.constraints.some(c=>c.kind==='on'&&corners.includes(c.point)&&sketch.entities.some(e=>e.id===c.curve&&e.kind==='circle')))continue;
  const at=corners.map(id=>point(sketch,id));
  const cx=at.reduce((s,p)=>s+p.x,0)/at.length, cy=at.reduce((s,p)=>s+p.y,0)/at.length;
  const radii=at.map(p=>Math.hypot(p.x-cx,p.y-cy)); const r=radii.reduce((s,v)=>s+v,0)/radii.length;
  if(!(r>1e-6)||radii.some(v=>Math.abs(v-r)>r*1e-3))continue;
  const centre=addPoint(sketch,cx,cy);
  const circle=addEntity(sketch,{kind:'circle',c:centre,r,construction:true});
  for(const id of corners)addConstraint(sketch,{kind:'on',point:id,curve:circle});
  // On the circle one side's axis fixes the turn; further ones would block resizing.
  const sides=new Set(loop.map(l=>l.id));
  const axes=sketch.constraints.filter(c=>(c.kind==='horizontal'||c.kind==='vertical')&&sides.has(c.line));
  const extra=new Set(axes.slice(1).map(c=>c.id));
  sketch.constraints=sketch.constraints.filter(c=>!extra.has(c.id));
 }
}
