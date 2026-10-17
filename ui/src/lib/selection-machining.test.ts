import { describe, expect, it } from 'vitest';
import type { Features, Leads } from '../api';
import { clearSelection, offEverywhere, onAnywhere, selectionView, setSelection } from './selection-machining';
import { toggled } from './features';

const leads = (side: Leads['side']): Leads => ({ entry: { shape: 'line', length: 2, radius: 1, angle: 45 }, exit: null, side, closed_only: true, clearance: null, overrides: [] });
const base = (): Features => ({
  leads: leads('auto'), joints: null, cooling: null, kerf: null, bridges: null, common: null,
  start: { position: 'automatic', direction: 'keep', spots: [{ contour: 4, fraction: 0.5 }] }, seam: 'seal',
  order: { strategy: 'as_drawn', inner_first: true, circles_first: false, spread_heat: false }, skip_layers: [],
} as unknown as Features);

describe('machining for a selection of contours', () => {
  it('changes the selected contours only and keeps the job and picked places', () => {
    const f = base();
    const view = structuredClone(selectionView(f, 1, null));
    view.leads!.side = 'inside';
    setSelection(f, [1, 2], 'leads', view);
    expect(f.leads!.side).toBe('auto');
    expect(selectionView(f, 1, null).leads!.side).toBe('inside');
    expect(selectionView(f, 2, null).leads!.side).toBe('inside');
    expect(selectionView(f, 3, null).leads!.side).toBe('auto');
    expect(selectionView(f, 1, null).start.spots).toEqual(f.start.spots);
  });

  it('adds a second setting to the same selection and splits a changed one', () => {
    const f = base();
    let view = structuredClone(selectionView(f, 1, null));
    view.leads = null;
    setSelection(f, [1, 2], 'leads', view);
    view = structuredClone(selectionView(f, 1, null));
    view.kerf = { width: 0.2, side: 'auto' };
    setSelection(f, [1, 2], 'kerf', view);
    expect(f.contour_machining).toHaveLength(1);
    expect(f.contour_machining![0]!.own.sort()).toEqual(['kerf', 'leads']);
    view = structuredClone(selectionView(f, 2, null));
    view.kerf = { width: 0.4, side: 'auto' };
    setSelection(f, [2], 'kerf', view);
    expect(f.contour_machining!.map((m) => m.contours)).toEqual([[1], [2]]);
    expect(selectionView(f, 2, null).leads).toBeNull();
    expect(selectionView(f, 2, null).kerf!.width).toBe(0.4);
    expect(selectionView(f, 1, null).kerf!.width).toBe(0.2);
  });

  it('gives a part back to the job without losing the others', () => {
    const f = base();
    let view = structuredClone(selectionView(f, 1, null));
    view.leads = null;
    setSelection(f, [1], 'leads', view);
    view = structuredClone(selectionView(f, 1, null));
    view.kerf = { width: 0.2, side: 'auto' };
    setSelection(f, [1], 'kerf', view);
    clearSelection(f, [1], 'leads');
    expect(selectionView(f, 1, null).leads!.side).toBe('auto');
    expect(selectionView(f, 1, null).kerf!.width).toBe(0.2);
    clearSelection(f, [1], 'kerf');
    expect(f.contour_machining).toEqual([]);
  });

  it('switching off for the job switches off layers and selections too', () => {
    const f = base();
    f.leads = null;
    f.layers = [{ name: 'A', mode: 'cut', color: null, machining: { leads: leads('auto'), joints: null, cooling: null, kerf: null, start: { position: 'keep', direction: 'keep', spots: [] }, seam: 'seal' } }];
    expect(onAnywhere(f, 'leads')).toBe(true);
    const off = toggled(f, 'leads', [], true);
    expect(off.leads).toBeNull();
    expect(off.layers![0]!.machining!.leads).toBeNull();
    expect(onAnywhere(off, 'leads')).toBe(false);
    const on = toggled(off, 'leads', [], true);
    expect(on.leads).not.toBeNull();
    const g = base();
    const view = structuredClone(selectionView(g, 1, null));
    setSelection(g, [1], 'leads', view);
    offEverywhere(g, 'leads');
    expect(g.contour_machining).toEqual([]);
  });
});
