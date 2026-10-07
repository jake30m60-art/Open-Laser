import { describe, expect, it } from 'vitest';
import { orderTrail, type PreviewSegment } from './cut-order';
const segments: PreviewSegment[] = [
 {a:[0,0],b:[10,0],length:10,contour:0,kind:'travel'},
 {a:[10,0],b:[20,0],length:10,contour:0,kind:'cut'},
 {a:[20,0],b:[30,0],length:10,contour:1,kind:'cut'},
];
describe('simulation trail colours',()=>{
 it('keeps laser-off travel separate and distinguishes engraved from cut lines',()=>{
  expect(orderTrail(segments,1,new Set([0]))).toEqual({moved:'M0 0L10 0',mark:'M10 0L20 0',cut:'M20 0L30 0'});
 });
 it('colours only the completed part of the current cut',()=>{
  expect(orderTrail(segments,25/30,new Set([0])).cut).toBe('M20 0L25 0');
  expect(orderTrail(segments,0,new Set([0]))).toEqual({cut:'',mark:'',moved:''});
 });
});
