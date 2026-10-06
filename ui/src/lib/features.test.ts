import { describe, expect, it } from 'vitest';
import { isOn, toggled } from './features';
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
