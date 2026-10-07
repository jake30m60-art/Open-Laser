import {beforeAll,expect,it} from 'vitest';
import {addConstraint,addEntity,addPoint,emptySketch,point} from './model';
import {extend} from './modify';
import {initSolver,solveEdit} from './solver';
beforeAll(async()=>{await initSolver();});
it('extends an end snapped to another line without dragging that line along',()=>{
 const s=emptySketch(),a=addPoint(s,0,0),b=addPoint(s,10,0),h=addEntity(s,{kind:'line',a,b});
 addConstraint(s,{kind:'horizontal',line:h});
 const c=addPoint(s,10,-10),d=addPoint(s,10,10),old=addEntity(s,{kind:'line',a:c,b:d});
 addConstraint(s,{kind:'vertical',line:old});addConstraint(s,{kind:'on',point:b,curve:old});
 const e=addPoint(s,25,-10),f=addPoint(s,25,10),target=addEntity(s,{kind:'line',a:e,b:f});
 addConstraint(s,{kind:'vertical',line:target});
 const extended=extend(s,h,{x:9,y:0});if('error'in extended)throw Error(extended.error);
 const out=solveEdit(s,extended.sketch);expect(out.ok).toBe(true);
 for(const id of [a,c,d,e,f]){expect(point(out.sketch,id).x).toBeCloseTo(point(s,id).x,5);expect(point(out.sketch,id).y).toBeCloseTo(point(s,id).y,5);}
 const line=out.sketch.entities.find(e=>e.id===h)!;if(line.kind!=='line')throw Error('line');
 expect(point(out.sketch,line.b).x).toBeCloseTo(25,5);
});
