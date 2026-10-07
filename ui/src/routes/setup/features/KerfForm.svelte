<script lang="ts">
  import { t } from '../../../i18n/index';

  // Kerf compensation: how wide the beam cuts, and which side is waste.
  import Chips from '../../../components/params/Chips.svelte';
  import Stepper from '../../../components/params/Stepper.svelte';
  import { defaultKerf } from '../../../lib/features';
  import type { Features, Kerf } from '../../../api';
  import type { SetFeatures } from './form';

  let { features, on, set }: { features: Features; on: boolean; set: SetFeatures } = $props();

  const SIDES = [['auto', t('Auto')], ['inside', t('Inside')], ['outside', t('Outside')]] as const;
  const kerf = $derived(features.kerf ?? defaultKerf());
  const setWidth = (v: number) => set((f) => { f.kerf!.width = v; });
  const setSide = (v: string) => set((f) => { f.kerf!.side = v as Kerf['side']; });
</script>

<Stepper label={t("Kerf width")} value={kerf.width} unit="mm" step={0.01} disabled={!on} apply={setWidth} />
<Chips label={t("Waste side")} options={SIDES} current={kerf.side} disabled={!on} apply={setSide} />
