import { describe, expect, it } from 'vitest';
import { pickArea, fillAreas, editableContours } from './viewer-geometry';
import { selectContour } from './picking';
import { simulationSegments, orderPosition } from './cut-order';
import type { Preview, PreviewContour } from '../api';
const box = (id: number, lo: number, hi: number, layer = '0'): PreviewContour => ({ sources:[id], layer, closed:true, lead_target:null, matching_contour:null, depth:0, start:[lo,lo], cooling:[], paths:[{kind:'cut',points:[[lo,lo],[hi,lo],[hi,hi],[lo,hi],[lo,lo]]}] });
describe('engraving connected faces and simulation', () => {
  it('chooses the ring or the inner face independently, including a nested island', () => {
    const outer=box(0,0,10), hole=box(1,2,8), island=box(2,4,6); const shapes=new Map([[0,[outer,hole,island]]]);
    expect(pickArea(shapes,[1,1])?.contour.sources).toEqual([0]);
    expect(pickArea(shapes,[3,3])?.contour.sources).toEqual([1]);
    expect(pickArea(shapes,[5,5])?.contour.sources).toEqual([2]);
    expect(fillAreas([outer,hole,island],[0,1,2])).toEqual([{contour:0,holes:[1]},{contour:1,holes:[2]},{contour:2,holes:[]}]);
    expect(pickArea(shapes,[3,3], c=>c.sources[0]!==1)?.contour.sources).toEqual([0]);
  });
  it('discards computed hatch as picking geometry and keeps the inner contour', () => {
    const outer=box(0,0,10,'Engraving'), inner=box(1,2,8);
    const region={...outer,paths:[...outer.paths,...inner.paths]};
    const scan={...outer,closed:false,paths:[{kind:'cut',points:[[0,1],[10,1]]}]} as PreviewContour;
    expect(editableContours([scan,inner],[region]).map(c=>[c.sources,c.paths.length])).toEqual([[[1],1],[[0],1]]);
  });
  it('selects one contour and toggles the final member off without expanding its part', () => {
    expect(selectContour([1,2],3,false)).toEqual([3]);
    expect(selectContour([3],3,false)).toEqual([]);
    expect(selectContour([1],2,true)).toEqual([1,2]);
    expect(selectContour([1,2],1,true)).toEqual([2]);
    expect(selectContour([2],2,true)).toEqual([]);
  });
  it('plays both operations or either one without confusing scan cuts with cutting', () => {
    const preview={contours:[box(0,0,10,'Engraving'),box(1,20,30)]} as Preview;
    expect(simulationSegments(preview,new Set(['Engraving']),'all')).toHaveLength(8);
    const cutting=simulationSegments(preview,new Set(['Engraving']),'cut');
    expect(cutting).toHaveLength(4); expect(cutting.every(s=>s.contour===1)).toBe(true);
    expect(orderPosition(cutting,1)?.point).toEqual([20,20]);
    expect(simulationSegments(preview,new Set(['Engraving']),'mark').every(s=>s.contour===0)).toBe(true);
  });
});
