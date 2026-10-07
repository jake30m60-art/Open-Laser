import { describe, expect, it } from 'vitest';
import { engravingFeatures } from './engraving-preview';
import { defaultFill } from './drawing-layers';
import type { DraftView } from '../api';
const identity: [number,number,number,number,number,number] = [1,0,0,1,0,0];
const draft = () => ({ features:{layers:[],layer_edits:[],skip_layers:[],fill_regions:[]}, layers:[{name:'0',mode:'cut'}], placed:[{source:0,copy:0,transform:identity},{source:1,copy:0,transform:[1,0,0,1,1,0]},{source:0,copy:1,transform:identity},{source:1,copy:1,transform:identity}],preview:{contours:[],engraving_regions:[]} }) as unknown as DraftView;
describe('staged engraving feature ownership', () => {
  it('keeps the cut hole independent and never changes the saved features', () => {
    const d=draft(), before=JSON.stringify(d); const f=engravingFeatures(d,[0],[{contour:0,holes:[1]}],defaultFill());
    expect(f.layer_edits).toEqual([{contour:0,layer:'Engraving'}]);
    expect(f.fill_regions).toEqual([{contour:0,holes:[1]},{contour:2,holes:[3]}]);
    expect(JSON.stringify(d)).toBe(before);
  });
  it('does not duplicate an area when multiple copies are selected', () => {
    const f=engravingFeatures(draft(),[0,2],[{contour:0,holes:[1]},{contour:2,holes:[3]}],defaultFill());
    expect(f.fill_regions).toHaveLength(2);
  });
  it('clears previously engraved owners and removes hatching for contour mode', () => {
    const d=draft(); d.layers.push({name:'Engraving',mode:'mark'} as never);
    d.preview!.contours.push({layer:'Engraving',sources:[0]} as never);
    const cleared=engravingFeatures(d,[],[],defaultFill());
    expect(cleared.layer_edits).toEqual([{contour:0,layer:'0'}]); expect(cleared.fill_regions).toEqual([]);
    const trace=engravingFeatures(d,[1],[],null);
    expect(trace.fill_regions).toEqual([]); expect(trace.layers!.find(l=>l.name==='Engraving')?.machining?.fill).toBeUndefined();
  });
});
