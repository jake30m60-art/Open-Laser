import { expect, it } from 'vitest';
import { createAction } from './actions';
import { addEntity, addPoint, emptySketch } from './model';

it('mirror keeps preselected curves editable until Enter explicitly selects the axis phase', () => {
  const sketch=emptySketch();
  const first=addEntity(sketch,{kind:'line',a:addPoint(sketch,2,0),b:addPoint(sketch,2,10)});
  const second=addEntity(sketch,{kind:'line',a:addPoint(sketch,4,0),b:addPoint(sketch,4,10)});
  const axis=addEntity(sketch,{kind:'line',a:addPoint(sketch,0,-20),b:addPoint(sketch,0,20)});
  const selection=new Set([first]);const ctx={sketch,selection};const action=createAction('mirror');
  const hit={point:null,entity:second,at:{x:4,y:5}};
  expect(action.click(ctx,hit,{})).toEqual({kind:'toggle',id:second});
  selection.add(second);
  expect(action.enter(ctx,{x:0,y:0},{}).kind).toBe('more');
  const axisHit={point:null,entity:axis,at:{x:0,y:0}};
  expect(action.preview(ctx,{x:0,y:0},axisHit,{}).shapes).toHaveLength(2);
  const result=action.click(ctx,axisHit,{});
  expect(result.kind).toBe('apply');
  if(result.kind==='apply')expect(result.sketch.entities).toHaveLength(5);
});
