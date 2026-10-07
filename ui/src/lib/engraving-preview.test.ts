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

 it('saving Gravur 2 keeps Gravur 1 and its holes and settings',()=>{
  const d=draft();d.features.layer_edits=[{contour:0,layer:'Gravur 1'}];
  d.layers.push({name:'Gravur 1',mode:'mark'} as never);
  d.preview!.engraving_regions=[{sources:[0],layer:'Gravur 1'} as never];
  d.features.fill_regions=[{contour:0,holes:[1]},{contour:2,holes:[3]}];
  const f=engravingFeatures(d,[1],[],null,'Gravur 2');
  expect(f.layer_edits).toContainEqual({contour:0,layer:'Gravur 1'});
  expect(f.layer_edits).toContainEqual({contour:1,layer:'Gravur 2'});
  expect(f.fill_regions).toEqual(d.features.fill_regions);
 });
 it('editing one engraving does not deselect an engraving on another layer',()=>{
  const d=draft();d.layers.push({name:'Gravur 1',mode:'mark'} as never,{name:'Gravur 2',mode:'mark'} as never);
  d.features.layer_edits=[{contour:0,layer:'Gravur 1'},{contour:1,layer:'Gravur 2'}];
  d.preview!.engraving_regions=[{sources:[0],layer:'Gravur 1'} as never,{sources:[1],layer:'Gravur 2'} as never];
  const f=engravingFeatures(d,[],[],defaultFill(),'Gravur 2');
  expect(f.layer_edits).toContainEqual({contour:0,layer:'Gravur 1'});
  expect(f.layer_edits).toContainEqual({contour:1,layer:'0'});
 });

it('clears saved owners even when output selection hides their prepared paths',()=>{
 const d=draft();d.layers.push({name:'Gravur 1',mode:'mark'} as never);
 d.preview!.source_contours=[{sources:[0],layer:'Gravur 1'},{sources:[1],layer:'Gravur 1'}] as never;
 const f=engravingFeatures(d,[],[],defaultFill(),'Gravur 1');
 expect(f.layer_edits).toContainEqual({contour:0,layer:'0'});
 expect(f.layer_edits).toContainEqual({contour:1,layer:'0'});
});
