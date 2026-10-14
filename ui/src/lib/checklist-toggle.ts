// The three checklists switched on or off in one tap, for both lasers at
// once; their steps stay as they are for when they come back.
import { api } from '../api/client';
import type { PreflightPreferences } from '../api';

export type ChecklistPhase = 'preflight' | 'pause' | 'postflight';

/** Whether a phase's checklist shows, for either laser. */
export function checklistOn(preferences: PreflightPreferences, phase: ChecklistPhase): boolean {
  const lists = phase === 'preflight' ? preferences : preferences[phase];
  return lists.fiber.enabled || lists.co2.enabled;
}

/** The preferences with a phase's checklist on or off for both lasers. */
export function withChecklist(preferences: PreflightPreferences, phase: ChecklistPhase, on: boolean): PreflightPreferences {
  const next = structuredClone(preferences);
  const lists = phase === 'preflight' ? next : next[phase];
  lists.fiber.enabled = on;
  lists.co2.enabled = on;
  return next;
}

/** Switches a phase's checklist on or off on the server, from its latest preferences. */
export async function setChecklist(phase: ChecklistPhase, on: boolean): Promise<PreflightPreferences> {
  const next = withChecklist(await api.preflightPreferences(), phase, on);
  await api.savePreflightPreferences(next);
  return next;
}
