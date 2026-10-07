import { beforeAll, expect, it } from 'vitest';
import { clone, emptySketch, point, addConstraint, addEntity, addPoint } from './model';
import { createTool } from './tools';
import { fillet, offset } from './modify';
import { initSolver, solveEdit } from './solver';
import { primaryRelations } from './primary-relations';
import { dist } from './geometry';
beforeAll(async()=>{await initSolver();});
it('edits 5 to 15 after solving the fillet creation at every rectangle corner',()=>{
 for (const toolId of ['rect2','rectc'] as const) for (let cornerIndex=0;cornerIndex<4;cornerIndex++) {
  const s=emptySketch(),t=createTool(toolId);
  t.click(s,{x:23,y:47,kind:'free'},{});t.click(s,{x:123,y:107,kind:'free'},{});
  const corner=s.points[cornerIndex]!; const f=fillet(s,corner.id,5); if('error'in f)throw Error(f.error);
  const created=solveEdit(s,f.sketch);expect(created.ok).toBe(true);
  const next=clone(created.sketch); const dim=next.constraints.find(c=>c.kind==='radius')!;
  if(dim.kind!=='radius')throw Error('radius');dim.value=15;
  const out=solveEdit(created.sketch,next);expect(out.ok).toBe(true);
  for(const p of created.sketch.points.filter(p=>s.points.some(old=>old.id===p.id))) {
   expect(point(out.sketch,p.id).x,`${toolId} corner ${cornerIndex} point ${p.id} x`).toBeCloseTo(p.x,5);
   expect(point(out.sketch,p.id).y,`${toolId} corner ${cornerIndex} point ${p.id} y`).toBeCloseTo(p.y,5);
  }
  const arc=out.sketch.entities.find(e=>e.kind==='arc')!;if(arc.kind!=='arc')throw Error('arc');
  expect(dist(point(out.sketch,arc.c),point(out.sketch,arc.a))).toBeCloseTo(15,5);
 }
});
it('rejects changing the right angle between rectangle sides to 45 degrees',()=>{
 const s=emptySketch(),t=createTool('rect2');t.click(s,{x:20,y:30,kind:'free'},{});t.click(s,{x:120,y:90,kind:'free'},{});
 const next=clone(s);addConstraint(next,{kind:'angle',a:s.entities[0]!.id,b:s.entities[1]!.id,value:45,at:[0,0]});
 expect(solveEdit(s,next).ok).toBe(false);
});
it('changes an attached line angle while preserving its length and the rectangle',()=>{
 const s=emptySketch(),t=createTool('rect2');t.click(s,{x:20,y:30,kind:'free'},{});t.click(s,{x:120,y:90,kind:'free'},{});
 const a=s.points[2]!.id,b=addPoint(s,150,120),line=addEntity(s,{kind:'line',a,b}); const oldLength=dist(point(s,a),point(s,b));
 const next=clone(s);addConstraint(next,{kind:'angle',a:s.entities[0]!.id,b:line,value:60,at:[0,0]});
 const out=solveEdit(s,next);expect(out.ok).toBe(true);
 for(const p of s.points.slice(0,4)){expect(point(out.sketch,p.id).x).toBeCloseTo(p.x,5);expect(point(out.sketch,p.id).y).toBeCloseTo(p.y,5);}
 expect(dist(point(out.sketch,a),point(out.sketch,b))).toBeCloseTo(oldLength,5);
});
it('edits one of four fillets without moving the other three or resizing the rectangle',()=>{
 const original=emptySketch(),t=createTool('rect2');t.click(original,{x:23,y:47,kind:'free'},{});t.click(original,{x:123,y:107,kind:'free'},{});
 let current=original;
 for(const p of original.points){const f=fillet(current,p.id,5);if('error'in f)throw Error(f.error);const out=solveEdit(current,f.sketch);expect(out.ok).toBe(true);current=out.sketch;}
 for(const dimension of current.constraints.filter(c=>c.kind==='radius')) {
  const next=clone(current);const dim=next.constraints.find(c=>c.id===dimension.id)!;if(dim.kind!=='radius')throw Error('radius');dim.value=15;
  const out=solveEdit(current,next);expect(out.ok).toBe(true);
  const arc=current.entities.find(e=>e.id===dim.curve)!;if(arc.kind!=='arc')throw Error('arc');
  for(const p of current.points.filter(p=>![arc.c,arc.a,arc.b].includes(p.id))){
   expect(point(out.sketch,p.id).x,`radius ${dim.id}, point ${p.id}`).toBeCloseTo(p.x,5);
   expect(point(out.sketch,p.id).y,`radius ${dim.id}, point ${p.id}`).toBeCloseTo(p.y,5);
  }
 }
});
it('rejects a fillet radius too large for its adjacent sides instead of flipping the contour',()=>{
 const s=emptySketch(),t=createTool('rect2');t.click(s,{x:20,y:30,kind:'free'},{});t.click(s,{x:60,y:50,kind:'free'},{});
 const f=fillet(s,s.points[2]!.id,5);if('error'in f)throw Error(f.error);const created=solveEdit(s,f.sketch);expect(created.ok).toBe(true);
 const next=clone(created.sketch),dim=next.constraints.find(c=>c.kind==='radius')!;if(dim.kind!=='radius')throw Error('radius');dim.value=30;
 expect(solveEdit(created.sketch,next).ok).toBe(false);
});
it('rotates the diagonal from the photo by sliding its end on the left edge',()=>{
 const s=emptySketch(),t=createTool('rect2');t.click(s,{x:20,y:30,kind:'free'},{});t.click(s,{x:120,y:110,kind:'free'},{});
 const right=s.points[2]!.id,end=addPoint(s,20,70),line=addEntity(s,{kind:'line',a:right,b:end}),left=s.entities[3]!.id;
 addConstraint(s,{kind:'on',point:end,curve:left});
 const next=clone(s);addConstraint(next,{kind:'angle',a:left,b:line,value:60,at:[0,0]});
 const out=solveEdit(s,next);expect(out.ok).toBe(true);
 for(const p of s.points.slice(0,4)){expect(point(out.sketch,p.id).x).toBeCloseTo(p.x,5);expect(point(out.sketch,p.id).y).toBeCloseTo(p.y,5);}
 expect(point(out.sketch,end).x).toBeCloseTo(20,5);
 expect(point(out.sketch,end).y).toBeCloseTo(110-100/Math.sqrt(3),5);
});
it('changes R5 to R12 and R15 with two rounded top corners and an inner circle',()=>{
 const s=emptySketch(),t=createTool('rect2');t.click(s,{x:-40,y:-30,kind:'free'},{});t.click(s,{x:40,y:30,kind:'free'},{});
 const center=addPoint(s,0,0),circle=addEntity(s,{kind:'circle',c:center,r:12});
 addConstraint(s,{kind:'diameter',curve:circle,value:24,at:[0,0]});
 let current=s; for(const id of [s.points[2]!.id,s.points[3]!.id]){
  const f=fillet(current,id,5);if('error'in f)throw Error(f.error);const out=solveEdit(current,f.sketch);expect(out.ok).toBe(true);current=out.sketch;
 }
 const radius=current.constraints.find(c=>c.kind==='radius')!;if(radius.kind!=='radius')throw Error('radius');
 for(const value of [12,15,5,15]){
  const before=current,next=clone(current),dim=next.constraints.find(c=>c.id===radius.id)!;if(dim.kind!=='radius')throw Error('radius');dim.value=value;
  const arc=before.entities.find(e=>e.id===radius.curve)!;if(arc.kind!=='arc')throw Error('arc');
  const out=solveEdit(before,next);expect(out.ok).toBe(true);
  for(const p of before.points.filter(p=>![arc.c,arc.a,arc.b].includes(p.id))){expect(point(out.sketch,p.id).x).toBeCloseTo(p.x,5);expect(point(out.sketch,p.id).y).toBeCloseTo(p.y,5);}
  current=out.sketch;
 }
});
it('refuses a diagonal angle fixed by both rectangle corners rather than stretching the rectangle',()=>{
 const s=emptySketch(),t=createTool('rect2');t.click(s,{x:0,y:0,kind:'free'},{});t.click(s,{x:100,y:60,kind:'free'},{});
 const diagonal=addEntity(s,{kind:'line',a:s.points[0]!.id,b:s.points[2]!.id});const next=clone(s);
 addConstraint(next,{kind:'angle',a:s.entities[0]!.id,b:diagonal,value:60,at:[0,0]});
 const out=solveEdit(s,next);expect(out.ok).toBe(false);expect(out.reason).toContain('Referenzkontur');
});

it('dimensions an offset rectangle without skewing it or altering its source',()=>{
 const original=emptySketch(),tool=createTool('rect2');
 tool.click(original,{x:0,y:0,kind:'free'},{});tool.click(original,{x:100,y:60,kind:'free'},{});
 const result=offset(original,original.entities.map(e=>e.id),5,{x:120,y:30});if('error' in result)throw Error(result.error);
 const base=result.sketch,edge=base.entities.find(e=>e.id===result.created[0])!;if(edge.kind!=='line')throw Error('line');
 const next=clone(base);addConstraint(next,{kind:'distance',a:edge.a,b:edge.b,axis:'aligned',value:120,at:[0,0]});
 const out=solveEdit(base,next);expect(out.ok).toBe(true);
 for(const old of original.points){expect(point(out.sketch,old.id).x).toBeCloseTo(old.x,5);expect(point(out.sketch,old.id).y).toBeCloseTo(old.y,5);}
 for(const e of out.sketch.entities.filter(e=>result.created.includes(e.id))){if(e.kind!=='line')continue;const a=point(out.sketch,e.a),b=point(out.sketch,e.b);expect(Math.min(Math.abs(a.x-b.x),Math.abs(a.y-b.y))).toBeLessThan(1e-5);}
 expect(dist(point(out.sketch,edge.a),point(out.sketch,edge.b))).toBeCloseTo(120,5);
});
it('does not infer midpoint or tangent relations in the default drawing mode',()=>{
 const s=emptySketch(),c=addPoint(s,0,0),circle=addEntity(s,{kind:'circle',c,r:10});
 const tool=createTool('line');tool.click(s,{x:20,y:0,kind:'free'},{});
 tool.click(s,{x:5,y:Math.sqrt(75),kind:'tangent',entity:circle,tangent:{curve:circle,at:'end',contact:{x:5,y:Math.sqrt(75)}}},{});
 expect(s.constraints.some(c=>['tangent','on','midpoint'].includes(c.kind))).toBe(false);
});

it('adds only missing primary axes when reopening an older unlinked offset',()=>{
 const s=emptySketch();const ids=[[0,0],[50,0],[50,30],[0,30]].map(([x,y])=>addPoint(s,x!,y!));
 for(let i=0;i<4;i++)addEntity(s,{kind:'line',a:ids[i]!,b:ids[(i+1)%4]!});
 const restored=primaryRelations(s);expect(restored.constraints.map(c=>c.kind).sort()).toEqual(['horizontal','horizontal','vertical','vertical']);
 expect(primaryRelations(restored).constraints).toEqual(restored.constraints);expect(s.constraints).toEqual([]);
 const e=restored.entities[0]!;if(e.kind!=='line')throw Error('line');const next=clone(restored);addConstraint(next,{kind:'distance',a:e.a,b:e.b,axis:'aligned',value:80,at:[0,0]});
 const out=solveEdit(restored,next);expect(out.ok).toBe(true);
 for(const edge of out.sketch.entities){if(edge.kind!=='line')continue;const a=point(out.sketch,edge.a),b=point(out.sketch,edge.b);expect(Math.min(Math.abs(a.x-b.x),Math.abs(a.y-b.y))).toBeLessThan(1e-5);}
});
