<script lang="ts">
  import { t } from '../../../i18n/index';

  // Lead-ins and lead-outs: the open tab picks which one is edited.
  import Chips from '../../../components/params/Chips.svelte';
  import Placed from '../../../components/params/Placed.svelte';
  import { featureEdits } from '../../../stores/feature-edits';
  import { ui } from '../../../stores/ui.svelte';
  import { explain } from '../../../lib/format';
  import Stepper from '../../../components/params/Stepper.svelte';
  import Toggle from '../../../components/params/Toggle.svelte';
  import { defaultLeads } from '../../../lib/features';
  import { plural } from '../../../lib/format';
  import { leadFor, setLeadOverrides } from '../../../lib/lead-overrides';
  import type { Features, Lead, Leads, Preview } from '../../../api';
  import type { PickFeature, SetFeatures } from './form';

  let { features, on, tab, selectedContours, set, pick, preview, whole = false }: {
    /** Editing the whole job, not a layer's or a selection's own. */
    whole?: boolean;
    features: Features;
    on: boolean;
    /** 0 edits the lead-in, 1 the lead-out. */
    tab: number;
    selectedContours: number[];
    set: SetFeatures;
    /** Starts picking places on the drawing, here the cut's start points. */
    pick: (feature: PickFeature) => void;
    /** The drawing's contours, where the selection's leads attach; none
     *  while a layer's own machining is edited. */
    preview: Preview | null;
  } = $props();

  const SHAPES = [['line', t('Line')], ['arc', 'Arc'], ['line_arc', t('Line and arc')]] as const;
  const SIDES = [['auto', t('Auto')], ['inside', t('Inside')], ['outside', t('Outside')]] as const;
  const defaultLead = (): Lead => ({ shape: 'line', length: 2, radius: 1, angle: 45 });
  // The seam belongs with the lead-out, where Mlaser keeps it: the same
  // setting the Start tool shows.
  const SEAMS = [['seal', t('Seal')], ['gap', 'Gap'], ['overcut', t('Overcut')]] as const;
  const seamKind = $derived(typeof features.seam === 'string' ? 'seal' : 'gap' in features.seam ? 'gap' : 'overcut');
  const setSeam = (v: string) => set((f) => { f.seam = v === 'seal' ? 'seal' : v === 'gap' ? { gap: 0.5 } : { overcut: 1 }; });
  const setSeamLength = (v: number) => set((f) => { f.seam = seamKind === 'gap' ? { gap: v } : { overcut: v }; });

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

  /** Layers and selections with their own leads that lack this one. */
  const shadowed = $derived(whole && leads[role] ? [...(features.layers ?? []).map(x => x.machining), ...(features.contour_machining ?? []).map(m => m.machining)]
    .filter(m => m?.leads && !m.leads[role]).length : 0);
  function switchLead(v: boolean): void {
    set((f) => {
      const l = f.leads ?? defaultLeads();
      if (tab === 0) l.entry = v ? defaultLead() : null;
      else l.exit = v ? defaultLead() : null;
      f.leads = l;
      // Job-wide, the switch also reaches layers and selections that keep
      // their own leads; they would otherwise hide it.
      if (whole) {
        const own = [...(f.layers ?? []).map(x => x.machining), ...(f.contour_machining ?? []).map(m => m.machining)];
        for (const m of own) if (m?.leads) m.leads[role] = v ? (m.leads[role] ?? defaultLead()) : null;
      }
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
  /** Chosen starts are the job's, whatever the leads apply to. */
  function clearStarts(): void {
    featureEdits.change((f) => { f.start.spots = []; }).catch((error) => ui.say(explain(error), true));
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
{#if shadowed}
  <p class="muted">{t("{0} layers or selections keep their own leads without this one.", shadowed)}</p>
  <button class="btn btn-soft" disabled={!on} onclick={() => switchLead(true)}>{t("Add it there too")}</button>
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
{#if tab === 1 && !local}
  <h4 class="lead-start">{t("Seam")}</h4>
  <Chips label={t("Where the cut meets its start")} options={SEAMS} current={seamKind} disabled={!on} apply={setSeam} />
  {#if typeof features.seam !== 'string'}
    {@const seam = features.seam}
    <Stepper label={'gap' in seam ? t('Gap') : t('Overcut')} value={'gap' in seam ? seam.gap : seam.overcut} unit="mm" step={0.1} disabled={!on} apply={setSeamLength} />
  {/if}
{/if}
{#if tab === 0}
  <h4 class="lead-start">{t("Start point")}</h4>
  <p class="muted lead-note">{t("Pick where a contour's cut starts; its lead-in attaches there.")}</p>
  <Placed count={features.start.spots.length} what="chosen start" onclear={clearStarts} onpick={() => pick('start')} />
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
  .lead-start { margin: 10px 0 2px; font-size: var(--t-sm); color: var(--ink-2); }
</style>
