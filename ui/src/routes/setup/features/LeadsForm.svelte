<script lang="ts">
  import { t } from '../../../i18n/index';

  // Lead-ins and lead-outs: the open tab picks which one is edited.
  import Chips from '../../../components/params/Chips.svelte';
  import Stepper from '../../../components/params/Stepper.svelte';
  import Toggle from '../../../components/params/Toggle.svelte';
  import { defaultLeads } from '../../../lib/features';
  import { plural } from '../../../lib/format';
  import { leadFor, setLeadOverrides } from '../../../lib/lead-overrides';
  import type { Features, Lead, Leads, Preview } from '../../../api';
  import type { SetFeatures } from './form';

  let { features, on, tab, selectedContours, set, preview }: {
    features: Features;
    on: boolean;
    /** 0 edits the lead-in, 1 the lead-out. */
    tab: number;
    selectedContours: number[];
    set: SetFeatures;
    /** The drawing's contours, where the selection's leads attach; none
     *  while a layer's own machining is edited. */
    preview: Preview | null;
  } = $props();

  const SHAPES = [['line', t('Line')], ['arc', 'Arc'], ['line_arc', t('Line and arc')]] as const;
  const SIDES = [['auto', t('Auto')], ['inside', t('Inside')], ['outside', t('Outside')]] as const;
  const defaultLead = (): Lead => ({ shape: 'line', length: 2, radius: 1, angle: 45 });

  const leads = $derived(features.leads ?? defaultLeads());
  const role = $derived(tab === 0 ? 'entry' : 'exit');
  /** Where the selected contours' leads attach. */
  const targets = $derived((preview?.contours ?? [])
    .filter((c) => c.lead_target && c.sources.some((i) => selectedContours.includes(i)))
    .map((c) => c.lead_target!));
  /** With contours selected, the form edits their leads alone. */
  let jobWide = $state(false);
  const local = $derived(!jobWide && targets.length > 0 && !!features.leads);
  const lead = $derived(local ? leadFor(leads, targets[0]!, role) : leads[role]);
  const selectionEdited = $derived(leads.overrides.some(edited => selectedContours.includes(edited.location.contour)));

  function switchLead(v: boolean): void {
    set((f) => {
      const l = f.leads ?? defaultLeads();
      if (tab === 0) l.entry = v ? defaultLead() : null;
      else l.exit = v ? defaultLead() : null;
      f.leads = l;
    });
  }
  /** Changes the lead on the open tab: the selection's, or the job's. */
  function edit(change: (l: Lead) => void): void {
    if (local) {
      const at = [...targets], which = role;
      set((f) => {
        const current = f.leads && leadFor(f.leads, at[0]!, which);
        if (!f.leads || !current) return;
        const next = structuredClone(current);
        change(next);
        setLeadOverrides(f.leads, at, which, next);
      });
      return;
    }
    set((f) => change(tab === 0 ? f.leads!.entry! : f.leads!.exit!));
  }
  const setShape = (v: string) => edit((l) => { l.shape = v as Lead['shape']; });
  const setLength = (v: number) => edit((l) => { l.length = v; });
  const setRadius = (v: number) => edit((l) => { l.radius = v; });
  const setAngle = (v: number) => edit((l) => { l.angle = v; });
  const setSide = (v: string) => set((f) => { f.leads!.side = v as Leads['side']; });
  const setClosedOnly = (v: boolean) => set((f) => { f.leads!.closed_only = v; });
  function useDefaultsForSelection(): void {
    set(f => { f.leads!.overrides = f.leads!.overrides.filter(edited => !selectedContours.includes(edited.location.contour)); });
  }
  function resetOverrides(): void {
    set(f => { f.leads!.overrides = []; });
  }
</script>

{#if targets.length && features.leads}
  <div class="seg block lead-scope" role="group" aria-label={t("Lead edits apply to")}>
    <button class:on={local} onclick={() => (jobWide = false)}>{t("Selected (")}{targets.length})</button>
    <button class:on={!local} onclick={() => (jobWide = true)}>{t("All contours")}</button>
  </div>
{/if}
{#if local && lead}
  <p class="muted lead-note">{t("Changes apply to the")} {plural(targets.length, 'selected contour')} {t("only. Switching a lead off or changing the side applies to all contours.")}</p>
{:else}
  <Toggle label={tab === 0 ? t('Lead-in') : t('Lead-out')} value={lead !== null} disabled={!on} apply={switchLead} />
{/if}
{#if lead}
  <Chips label={t("Shape")} options={SHAPES} current={lead.shape} disabled={!on} apply={setShape} />
  {#if lead.shape !== 'arc'}<Stepper label={t("Length")} value={lead.length} unit="mm" step={0.5} disabled={!on} apply={setLength} />{/if}
  {#if lead.shape !== 'line'}<Stepper label={t("Arc radius")} value={lead.radius} unit="mm" step={0.5} disabled={!on} apply={setRadius} />{/if}
  <Stepper label={lead.shape === 'arc' ? t('Arc sweep') : t('Angle')} value={lead.angle} unit="°" step={15} disabled={!on} apply={setAngle} />
{/if}
{#if !local}
  <Chips label={t("Side")} options={SIDES} current={leads.side} disabled={!on} apply={setSide} />
  <Toggle label={t("Closed contours only")} value={leads.closed_only} disabled={!on} apply={setClosedOnly} />
{/if}
{#if leads.overrides?.length}
  <p class="muted" style="font-size:var(--t-sm)">{plural(leads.overrides.length, 'contour')} {t("with individual lead settings")}</p>
  <button class="btn btn-soft" disabled={!on || !selectionEdited} onclick={useDefaultsForSelection}>{t("Use job defaults for selection")}</button>
  <button class="btn btn-ghost" disabled={!on} onclick={resetOverrides}>{t("Reset all individual leads")}</button>
{/if}

<style>
  .lead-scope { margin: 4px 0 8px; }
  .lead-scope button { flex: 1; }
  .lead-note { font-size: var(--t-sm); margin: 0 0 6px; }
</style>
