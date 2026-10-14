import { describe, expect, it } from 'vitest';
import { checklistOn, withChecklist } from './checklist-toggle';
import type { PreflightPreferences } from '../api';

const list = (enabled: boolean) => ({ enabled, steps: [{ text: 'Check', action: { kind: 'none' } }] }) as never;
const prefs = (): PreflightPreferences => ({
  version: 1, revision: 3, fiber: list(true), co2: list(false), confirm_gas: true,
  postflight: { fiber: list(true), co2: list(true) }, pause: { fiber: list(true), co2: list(true) },
}) as PreflightPreferences;

describe('checklist switches', () => {
  it('turns a phase off for both lasers and keeps its steps and the others', () => {
    const off = withChecklist(prefs(), 'pause', false);
    expect(checklistOn(off, 'pause')).toBe(false);
    expect(off.pause.fiber.steps).toHaveLength(1);
    expect(checklistOn(off, 'preflight')).toBe(true);
    expect(checklistOn(off, 'postflight')).toBe(true);
    expect(off.confirm_gas).toBe(true);
  });
  it('turns the preflight checklist back on for both lasers', () => {
    const on = withChecklist(withChecklist(prefs(), 'preflight', false), 'preflight', true);
    expect(on.fiber.enabled && on.co2.enabled).toBe(true);
  });
});
