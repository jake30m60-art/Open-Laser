import { describe, expect, it } from 'vitest';
import { isOn, toggled, toolbarState } from './features';
import type { Features } from '../api';

describe('toggled', () => {
  it('switches a flying cut off again when merged into the draft', () => {
    // Callers merge the result with Object.assign, which keeps deleted keys.
    const features = { common: null } as unknown as Features;
    Object.assign(features, toggled(features, 'fly', [1, 2]));
    expect(isOn(features, 'fly')).toBe(true);
    Object.assign(features, toggled(features, 'fly'));
    expect(isOn(features, 'fly')).toBe(false);
    expect(JSON.parse(JSON.stringify(features)).fly).toBeUndefined();
  });
});

it('reports a layer entry lead when the job default is off',()=>{
 const f={leads:null,layers:[{name:'Cut',mode:'cut',machining:{leads:{entry:{shape:'line',length:2}}}}]} as unknown as Features;
 expect(toolbarState(f,'leads',['Cut'])).toContain('Cut:');
 expect(toolbarState(f,'leads',['Cut'])).not.toBe('Off');
});
