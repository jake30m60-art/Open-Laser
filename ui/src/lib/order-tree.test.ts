import { it, expect } from 'vitest';
import { moveOrderRow, orderRows } from './order-tree';
import type { Preview } from '../api';
it('collapses engraving chunks without losing bridged sources', () => {
  const p={contours:[{sources:[0],layer:'Mark',depth:0,closed:false},{sources:[0],layer:'Mark',depth:0,closed:false},{sources:[1,2],layer:'Cut',depth:1,closed:true},{sources:[3],layer:'Cut',depth:0,closed:true}]} as unknown as Preview;
  const rows=orderRows(p); expect(rows.map(r=>r.sources)).toEqual([[0],[1,2],[3]]);
  expect(moveOrderRow(rows,2,1)).toEqual([0,3,1,2]);
  expect(moveOrderRow(rows,0,1)).toBeNull();
  expect(moveOrderRow(rows,-1,1)).toBeNull();
});
