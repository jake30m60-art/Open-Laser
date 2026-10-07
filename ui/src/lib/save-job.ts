import { t } from '../i18n/index';
// Saving the open job, from Setup on either screen: a name, then the merge
// review when it would overwrite saved changes. Unsaved numbered sheets save
// together as a folder of jobs.
import { api } from '../api/client';
import { osk } from './osk.svelte';
import { plural } from './format';
import { ui } from '../stores/ui.svelte';
import type { DraftView, JobView } from '../api';

type Draft = Pick<DraftView, 'name' | 'sheets' | 'preflight'>;

/** Numbered sheets not yet saved: they save as one folder. */
const numbered = (draft: Draft): boolean => draft.sheets?.pages.some((p) => !p.job) ?? false;

export const saveLabel = (draft: Draft): string =>
  numbered(draft) ? t('Save {0}', plural(draft.sheets!.pages.length, 'sheet')) : t('Save job');

export const preflightLabel = (draft: Draft): string =>
  draft.preflight.kind === 'inherit' ? t('Mode defaults')
  : draft.preflight.kind === 'off' ? t('Checklist off')
  : plural(draft.preflight.steps.length, 'custom check');

/** Asks for a name and saves; `run` reports failures and holds the screen busy. */
export function saveJob(draft: Draft, job: JobView | null | undefined, run: (action: () => Promise<void>) => void): void {
  const batch = numbered(draft);
  const total = draft.sheets?.pages.length ?? 1;
  osk.text(batch ? 'Folder for numbered sheets' : 'Job name', job?.name ?? (draft.name || 'job'), (typed) => {
    const name = typed.trim();
    if (!name) return;
    run(async () => {
      if (batch) {
        await api.saveJob(name);
        ui.say(t('Saved {0} numbered sheets in {1}.', total, name));
        return;
      }
      const review = await api.mergeReview(name);
      if (review.conflicts.length) {
        ui.pendingJobName = name;
        ui.modal = 'pending';
        return;
      }
      await api.saveJob(review.name);
      ui.say(t('Saved job {0}.', review.name));
    });
  });
}
