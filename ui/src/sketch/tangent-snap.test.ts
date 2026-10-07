import {beforeAll,expect,it} from 'vitest';
import {addConstraint,addEntity,addPoint,emptySketch,point} from './model';
import {snap} from './snap';
import {createTool} from './tools';
import {dist} from './geometry';
import {initSolver,solve} from './solver';
beforeAll(async()=>{await initSolver();});
function circle(){const s=emptySketch(),c=addPoint(s,0,0),id=addEntity(s,{kind:'circle',c,r:10});return {s,c,id};}
it('finds both tangent contacts from an outside point before the grid or on-curve snap',()=>{
 const {s,id}=circle();for(const sign of [-1,1]) {
  const sn=snap(s,{x:5.1,y:sign*8.7},{reach:0.5,grid:1,anchor:{x:20,y:0},tangent:true});
  expect(sn.kind).toBe('tangent');expect(sn.tangent?.curve).toBe(id);expect(sn.x).toBeCloseTo(5,8);expect(sn.y).toBeCloseTo(sign*Math.sqrt(75),8);
 }
});
it('draws a tangent line with a contact constraint and keeps tangency when the circle grows',()=>{
 const {s,c,id}=circle();addConstraint(s,{kind:'fix',point:c});
 const tool=createTool('line',true);tool.click(s,{x:20,y:0,kind:'free'},{});
 const end=snap(s,{x:5.1,y:8.7},{reach:0.5,grid:1,anchor:tool.anchor(),tangent:true});tool.click(s,tool.resolve(end,{}),{});
 expect(s.constraints.filter(c=>c.kind==='tangent')).toHaveLength(1);expect(s.constraints.filter(c=>c.kind==='on')).toHaveLength(1);
 const solved=solve(s);expect(solved.ok).toBe(true);expect(solved.redundant).toEqual([]);
 addConstraint(s,{kind:'diameter',curve:id,value:24,at:[0,0]});const grown=solve(s);expect(grown.ok).toBe(true);
 const line=grown.sketch.entities.find(e=>e.kind==='line')!;if(line.kind!=='line')throw Error('line');
 const a=point(grown.sketch,line.a),b=point(grown.sketch,line.b),center=point(grown.sketch,c);
 expect(Math.abs((b.x-a.x)*(center.y-a.y)-(b.y-a.y)*(center.x-a.x))/dist(a,b)).toBeCloseTo(12,6);
});
it('continues tangentially from a circle and preserves a typed length',()=>{
 const {s}=circle();const tool=createTool('line',true);tool.click(s,snap(s,{x:10,y:0},{reach:0.5,grid:0}),{});
 const sn=snap(s,{x:10.1,y:20},{reach:0.5,grid:1,anchor:tool.anchor(),tangent:true});expect(sn.kind).toBe('tangent');expect(sn.tangent?.at).toBe('start');
 const resolved=tool.resolve(sn,{length:30});expect(resolved.kind).toBe('tangent');expect(resolved.y).toBeCloseTo(30,8);
 tool.click(s,resolved,{});expect(s.constraints.filter(c=>c.kind==='on')).toHaveLength(1);expect(s.constraints.filter(c=>c.kind==='tangent')).toHaveLength(1);
 const out=solve(s);expect(out.ok).toBe(true);expect(out.redundant).toEqual([]);
});
it('respects an explicit angle that overrides the tangent direction',()=>{
 const {s}=circle();const tool=createTool('line',true);tool.click(s,snap(s,{x:10,y:0},{reach:0.5,grid:0}),{});
 const sn=snap(s,{x:10.1,y:20},{reach:0.5,grid:1,anchor:tool.anchor(),tangent:true});const resolved=tool.resolve(sn,{angle:45,length:20});
 expect(resolved.tangent).toBeUndefined();tool.click(s,resolved,{});expect(s.constraints.some(c=>c.kind==='tangent')).toBe(false);
});
it('does not infer tangent contacts inside a circle or when the line tool is not active',()=>{
 const {s}=circle();expect(snap(s,{x:5,y:8.6},{reach:0.5,grid:0,anchor:{x:1,y:1},tangent:true}).kind).not.toBe('tangent');
 expect(snap(s,{x:5,y:8.6},{reach:0.5,grid:0,anchor:{x:20,y:0}}).kind).not.toBe('tangent');
});
it('only offers contacts within the arc and shares a tangent endpoint without redundant constraints',()=>{
 const s=emptySketch(),c=addPoint(s,0,0),a=addPoint(s,10,0),b=addPoint(s,0,10),arc=addEntity(s,{kind:'arc',c,a,b});
 const above=snap(s,{x:5,y:8.66},{reach:0.5,grid:0,anchor:{x:20,y:0},tangent:true});expect(above.kind).toBe('tangent');
 const below=snap(s,{x:5,y:-8.66},{reach:0.5,grid:0,anchor:{x:20,y:0},tangent:true});expect(below.kind).not.toBe('tangent');
 const tool=createTool('line',true);tool.click(s,{x:20,y:10,kind:'free'},{});const end=snap(s,{x:0,y:10},{reach:0.5,grid:0,anchor:tool.anchor(),tangent:true});
 expect(end.kind).toBe('tangent');expect(end.point).toBe(b);tool.click(s,end,{});
 expect(s.constraints.some(k=>k.kind==='tangent' && k.b===arc)).toBe(true);expect(s.constraints.some(k=>k.kind==='on')).toBe(false);
 const out=solve(s);expect(out.ok).toBe(true);expect(out.redundant).toEqual([]);
});
