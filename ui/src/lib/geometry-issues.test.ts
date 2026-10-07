import { expect, it } from 'vitest';
import { issueSources, excludedSources } from './geometry-issues';
import { channelBand } from './bridge-preview';
import { orderSegments, simulationDistanceProgress, simulationTimeProgress, simulationSteps } from './cut-order';
import type { Preview } from '../api';
it('locates only reported placed contours and removes their raw outline from playback', () => {
  expect([...issueSources('contour 41: crosses itself')]).toEqual([41]);
  expect([...issueSources('contours 2 and 5 overlap')]).toEqual([2,5]);
  expect([...issueSources('bridge 2: endpoints cross')]).toEqual([]);
  const p={contours:[{sources:[41],paths:[{kind:'cut',points:[[0,0],[10,0]]}]},{sources:[3],paths:[{kind:'cut',points:[[0,0],[5,0]]}]}]} as Preview;
  expect(orderSegments(p,issueSources('contour 41: crosses itself')).map(s=>s.contour)).toEqual([1]);
});
it('shows a bridge at its actual drawing Y with the requested channel width', () => {
  expect(channelBand([0,-5],[10,-5],2)).toBe('M0.00 -4.00L10.00 -4.00L10.00 -6.00L0.00 -6.00Z');
  expect(channelBand([0,0],[0,0],2)).toBe('');
});

it('keeps a short cut visible even after a thousand-times-longer engraving', () => {
  const segments=[{a:[0,0],b:[1000,0],contour:0,length:1000,kind:'cut'},{a:[0,0],b:[1,0],contour:1,length:1,kind:'cut'}] as ReturnType<typeof orderSegments>;
  const layer=(i:number)=>i===0?'Engraving':'0';
  expect(simulationDistanceProgress(segments,.5,layer)).toBeCloseTo(1000/1001);
  expect(simulationDistanceProgress(segments,.75,layer)).toBeCloseTo(1000.5/1001);
  expect(simulationTimeProgress(segments,1000.5/1001,layer)).toBeCloseTo(.75);
});

it('numbers engraving chunks once and keeps the following cut as a separate operation', () => {
  const p={contours:[{layer:'Engraving',sources:[0],paths:[{kind:'cut',points:[[0,0],[10,0]]}]},{layer:'Engraving',sources:[0],paths:[{kind:'cut',points:[[0,1],[10,1]]}]},{layer:'0',sources:[1],paths:[{kind:'cut',points:[[0,0],[1,0]]}]}]} as Preview;
  const steps=simulationSteps(p,orderSegments(p));
  expect(steps.map(s=>[s.layer,s.sources,s.contours,s.distance])).toEqual([['Engraving',[0],[0,1],0],['0',[1],[2],20]]);
});

it('never plays a raw outline outside the explicitly selected output scope', () => {
  expect([...excludedSources(null,[3],[0,3,41])]).toEqual([0,41]);
  expect([...excludedSources('contour 41: crosses itself',null,[0,3,41])]).toEqual([41]);
});
