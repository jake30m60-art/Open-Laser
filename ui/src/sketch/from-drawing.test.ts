import { it, expect } from 'vitest';
import { fromDrawing } from './from-drawing';
import { point } from './model';
import { toDxf } from './dxf';
it('edits exact source lines, clockwise arcs and circles, retaining their layers', () => {
  const s=fromDrawing({contours:[{layer:'Cut',curves:[{type:'line',start:{x:0,y:0},end:{x:5,y:0}},{type:'line',start:{x:5,y:0},end:{x:5,y:5}}]},{layer:'Mark',curves:[{type:'arc',center:{x:10,y:10},radius:2,start_angle:0,sweep:-Math.PI/2},{type:'arc',center:{x:20,y:20},radius:3,start_angle:0,sweep:Math.PI*2}]}]});
  expect(s.entities.map(e=>e.kind)).toEqual(['line','line','arc','circle']);
  const first=s.entities[0]!,second=s.entities[1]!; if(first.kind==='line' && second.kind==='line') expect(first.b).toBe(second.a);
  const arc=s.entities[2]!; if(arc.kind==='arc') { expect(point(s,arc.a).y).toBeCloseTo(8); expect(point(s,arc.b).x).toBeCloseTo(12); }
  const dxf=toDxf(s); expect(dxf).toContain('8\r\nCut\r\n'); expect(dxf).toContain('8\r\nMark\r\n'); expect(dxf).toContain('CIRCLE');
});

it('does not link coincident endpoints of independent contours',()=>{
 const curve={type:'line' as const,start:{x:0,y:0},end:{x:5,y:0}}; const s=fromDrawing({contours:[{layer:'Cut',curves:[curve]},{layer:'Mark',curves:[curve]}]});
 const a=s.entities[0]!,b=s.entities[1]!; if(a.kind==='line'&&b.kind==='line')expect(a.a).not.toBe(b.a);
});
