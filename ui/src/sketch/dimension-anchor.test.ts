import { beforeAll, expect, it } from 'vitest';
import { addConstraint, addEntity, addPoint, clone, emptySketch, point } from './model';
import { createTool } from './tools';
import { dist } from './geometry';
import { initSolver, solve, solveEdit } from './solver';
beforeAll(async () => { await initSolver(); });
it('resizes a rectangle from its original first corner when adding and editing its width', () => {
  const s = emptySketch(), tool = createTool('rect2');
  tool.click(s, { x: 20, y: 30, kind: 'free' }, {});
  tool.click(s, { x: 60, y: 50, kind: 'free' }, {});
  const line = s.entities[0]!; if (line.kind !== 'line') throw new Error('line');
  const next = clone(s);
  const id = addConstraint(next, { kind:'distance', a:line.b, b:line.a, axis:'horizontal', value:100, at:[0,0] });
  const first = solveEdit(s, next); expect(first.ok).toBe(true);
  expect(point(first.sketch,line.a).x).toBeCloseTo(20, 5);
  expect(point(first.sketch,line.a).y).toBeCloseTo(30, 5);
  expect(dist(point(first.sketch,line.a), point(first.sketch,line.b))).toBeCloseTo(100,5);
  const edited = clone(first.sketch); const dim = edited.constraints.find(c => c.id === id)!;
  if (dim.kind !== 'distance') throw new Error('dimension'); dim.value=150;
  const result=solveEdit(first.sketch,edited); expect(result.ok).toBe(true);
  expect(point(result.sketch,line.a).x).toBeCloseTo(20,5);
  expect(point(result.sketch,line.a).y).toBeCloseTo(30,5);
  expect(dist(point(result.sketch,line.a),point(result.sketch,line.b))).toBeCloseTo(150,5);
  expect(result.sketch.constraints).toEqual(edited.constraints);
  expect(solve(result.sketch).dof).toBeGreaterThan(0);
});
it('keeps the earlier separate line when dimensioning a gap in reverse selection order', () => {
  const s=emptySketch(); const a=addPoint(s,10,20), b=addPoint(s,10,50);
  addEntity(s,{kind:'line',a,b}); const c=addPoint(s,40,20), d=addPoint(s,40,50);
  addEntity(s,{kind:'line',a:c,b:d}); const next=clone(s);
  addConstraint(next,{kind:'distance',a:c,b:a,axis:'horizontal',value:80,at:[0,0]});
  const out=solveEdit(s,next); expect(out.ok).toBe(true);
  expect(point(out.sketch,a).x).toBeCloseTo(10,5); expect(point(out.sketch,b).x).toBeCloseTo(10,5);
  expect(point(out.sketch,b).y).toBeCloseTo(50,5); expect(point(out.sketch,c).x).toBeCloseTo(90,5);
});
it('keeps a circle centre while changing its diameter', () => {
  const s=emptySketch(), c=addPoint(s,25,35), circle=addEntity(s,{kind:'circle',c,r:5});
  const next=clone(s); addConstraint(next,{kind:'diameter',curve:circle,value:30,at:[0,0]});
  const out=solveEdit(s,next); expect(out.ok).toBe(true);
  expect(point(out.sketch,c).x).toBeCloseTo(25,5); expect(point(out.sketch,c).y).toBeCloseTo(35,5);
  expect(out.sketch.entities[0]).toMatchObject({r:15});
});
it('allows the earlier reference to move when an explicit fix requires it', () => {
  const s=emptySketch(), a=addPoint(s,10,20), b=addPoint(s,40,20);
  addEntity(s,{kind:'line',a,b}); addConstraint(s,{kind:'fix',point:b});
  const next=clone(s); addConstraint(next,{kind:'distance',a,b,axis:'horizontal',value:80,at:[0,0]});
  const out=solveEdit(s,next); expect(out.ok).toBe(true);
  expect(point(out.sketch,b).x).toBeCloseTo(40,5); expect(point(out.sketch,a).x).toBeCloseTo(-40,5);
});
it('keeps the first line for a line-to-line offset, in either selection order', () => {
  for (const reverse of [false,true]) {
    const s=emptySketch(), a=addPoint(s,10,20), b=addPoint(s,10,50);
    const first=addEntity(s,{kind:'line',a,b}), c=addPoint(s,40,20), d=addPoint(s,40,50);
    const second=addEntity(s,{kind:'line',a:c,b:d});
    addConstraint(s,{kind:'vertical',line:first}); addConstraint(s,{kind:'vertical',line:second});
    const next=clone(s); addConstraint(next,{kind:'offset',point:reverse?a:c,line:reverse?second:first,value:80,at:[0,0]});
    const out=solveEdit(s,next); expect(out.ok).toBe(true);
    expect(point(out.sketch,a).x).toBeCloseTo(10,5); expect(point(out.sketch,a).y).toBeCloseTo(20,5);
    expect(point(out.sketch,b).x).toBeCloseTo(10,5); expect(point(out.sketch,b).y).toBeCloseTo(50,5);
    expect(point(out.sketch,c).x).toBeCloseTo(90,5); expect(point(out.sketch,d).x).toBeCloseTo(90,5);
  }
});
it('preserves the entire older rectangle when dimensioning between two objects', () => {
  const s=emptySketch();
  for (const [x,y] of [[10,20],[100,20]]) {
    const tool=createTool('rect2'); tool.click(s,{x:x!,y:y!,kind:'free'},{});
    tool.click(s,{x:x!+30,y:y!+40,kind:'free'},{});
  }
  const earlier=s.entities[1]!, later=s.entities[7]!;
  if (earlier.kind !== 'line' || later.kind !== 'line') throw new Error('lines');
  const next=clone(s); addConstraint(next,{kind:'offset',point:later.a,line:earlier.id,value:100,at:[0,0]});
  const out=solveEdit(s,next); expect(out.ok).toBe(true);
  for (const p of s.points.slice(0,4)) {
    expect(point(out.sketch,p.id).x).toBeCloseTo(p.x,5);
    expect(point(out.sketch,p.id).y).toBeCloseTo(p.y,5);
  }
});
