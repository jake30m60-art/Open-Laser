import { describe,it,expect } from 'vitest';
import { defaultTemplate, referenceDrawing, nearestPick, pointOn, mapped } from './workpiece-alignment';
describe('alignment reference selection',()=>{
  it('keeps clicked locations on their actual edge, without shifting drawing origin',()=>{
    const t=defaultTemplate(),d=referenceDrawing(t.shape),before=JSON.stringify(d);
    const selected=nearestPick(d,[120,-1])!;
    expect(selected.pick).toEqual({contour:0,curve:0,t:0.2});
    expect(pointOn(d.contours[0]!.curves[0]!,selected.pick.t)).toEqual([120,0]);
    mapped([120,0],{translation:[30,40],angle:0.1});expect(JSON.stringify(d)).toBe(before);
  });
  it('preserves a special reference and its holes without adding machining contours',()=>{
    const outer=referenceDrawing({kind:'rectangle',width:100,height:80});
    const hole=referenceDrawing({kind:'disc',diameter:10});
    const d={contours:[...outer.contours,...hole.contours]};
    expect(referenceDrawing({kind:'special',drawing:d})).toBe(d);
    expect(nearestPick(d,[-5,0])!.pick.contour).toBe(1);
  });
  it('snaps a clockwise arc correctly',()=>{
    const curve={type:'arc' as const,center:{x:0,y:0},radius:10,start_angle:0,sweep:-Math.PI};
    expect(nearestPick({contours:[{layer:'',curves:[curve]}]},[0,-10])!.pick.t).toBeCloseTo(0.5);
  });
});
