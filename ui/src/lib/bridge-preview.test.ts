import {it,expect} from 'vitest';
import {snapBridge,bridgeOpening} from './bridge-preview';
import {editableContours,pickArea} from './viewer-geometry';
import type {PreviewContour} from '../api';
const c={closed:true,sources:[0,1],layer:'Cut',paths:[{kind:'cut',points:[[0,0],[10,0],[10,10],[0,10],[0,0]]}]} as PreviewContour;
it('snaps to an edge within the tolerance and rejects distant points',()=>{
 expect(snapBridge([c],[5,-0.5],1)?.point).toEqual([5,0]);
 expect(snapBridge([c],[5,-2],1)).toBeNull();
 expect(bridgeOpening([c],[5,0],2)).toBe('M4.00 0.00L6.00 0.00');
});
it('a bridged closed contour remains selectable as one connected area',()=>{
 expect(editableContours([c],[])).toEqual([c]);
 expect(pickArea(new Map([[0,[c]]]),[5,5])?.contour.sources).toEqual([0,1]);
});
