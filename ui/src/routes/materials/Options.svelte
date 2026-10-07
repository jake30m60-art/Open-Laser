<script lang="ts">
  import { t } from '../../i18n/index';

  import { plural, recipeLabel } from '../../lib/format';
  // The Process options page: the optional behaviours as cards, each with
  // its state and a line on what it does, in the vendor's order. A card
  // opens its sheet.
  import { bare, shown, cutStart, headMode, isOn, mode, type Editor } from '../../lib/recipe';
  import type { RecipeView } from '../../api';

  let { ed, film, onopen }: { ed: Editor; /** The film process the recipe refers to, as staged. */ film: RecipeView | null; onopen: (sheet: string) => void } = $props();
  const v = $derived(ed.values);
  const on = (key: string) => isOn(v[key]);
  const mm = (key: string) => shown(key, v[key] ?? '');
  const m = $derived(mode(v));
  const start = $derived(cutStart(v));
  const caps = $derived(ed.a.caps);
  type Card = { id: string; title: string; status: string; text: string };
  const cards = $derived.by((): Card[] => {
    const head = headMode(v);
    const list: Card[] = [
      {
        id: 'start', title: t("Start work Segment"), status: start.enabled ? 'On' : 'Off',
        text: start.enabled
          ? `${shown('UD_UpLen', start.length)} at ${shown('UD_UpSpeed', start.speed)}${start.legacy ? ' · legacy slow start' : ''}`
          : t('Use the main cutting settings'),
      },
      {
        id: 'end', title: t("End work Segment"), status: on('UD_DownEnable') ? 'On' : 'Off',
        text: on('UD_DownEnable')
          ? `${mm('UD_DownLen')} at ${shown('UD_DownSpeed', v['UD_DownSpeed'] ?? '')}`
          : t('Use the main cutting settings'),
      },
      {
        id: 'timing', title: t('Delay Parameters'), status: `${bare('LaserOnDelay', v['LaserOnDelay'] ?? '0')} ms`,
        text: t('Start dwell · end dwell · after-off wait'),
      },
      {
        id: 'head', title: t('Head movement'),
        status: head === 'fixed' ? t('Fixed height')
          : head === 'absolute' ? t('Absolute height')
          : on('NoFollow') ? t('Following off')
          : t('Following'),
        text: `${mm('UpHeight')} retract · ${on('ShortDistNoUp') ? 'keep height on short moves' : 'lift between contours'}`
          + `${ed.a.gas && (on('ShortDistGasKeepOn') || on('NoCloseGasInManu')) ? ' · gas kept' : ''}`,
      },
      {
        id: 'curves', title: t("Dynamic Power"),
        status: on('PowerAdjustWithSpeed') || on('FreqAdjustWithSpeed') ? 'On' : 'Off',
        text: t('Adjust Cut Power and frequency as the speed changes'),
      },
      {
        id: 'residue', title: t("Clean Residue"),
        status: on('CleanResidue_Enable') ? (m === 'staged' ? 'On' : 'Inactive') : 'Off',
        text: on('CleanResidue_Enable')
          ? `${mm('CleanResidue_WorkR')} radius · ${bare('CleanResidue_SpiralTimes', v['CleanResidue_SpiralTimes'] ?? '')} turns`
            + `${m === 'staged' ? '' : ' · runs only after staged piercing'}`
          : 'A separate cleaning motion after piercing',
      },
      {
        id: 'pre', title: t("PreDrill"),
        status: on('PreDrill') ? (m === 'staged' ? 'On' : 'Inactive') : 'Off',
        text: on('PreDrill')
          ? `${caps?.pre_pierce_batch == null ? '? contours' : plural(caps.pre_pierce_batch, 'contour')} per batch`
            + `${m === 'staged' ? '' : ' · needs staged piercing'}`
          : t('Pierce a batch of contours before cutting them'),
      },
      {
        id: 'film', title: t("With Film"), status: on('WithFilm') ? 'On' : 'Off',
        text: on('WithFilm')
          ? (film ? t('Film process: {0}', recipeLabel(film)) : t('Choose a film process'))
          : t('Run a separate pass before the cut'),
      },
      {
        id: 'shift', title: t('Graph Shift'), status: on('EnableContourShift') ? 'On' : 'Off',
        text: on('EnableContourShift') ? `X ${mm('ContourShiftXDist')} · Y ${mm('ContourShiftYDist')}` : t('Apply an explicit X / Y offset'),
      },
    ];
    return list;
  });
</script>

<div class="section-head"><div><h2>{t("Process options")}</h2><p class="muted">{t("Optional behaviour, with the active settings visible at a glance.")}</p></div></div>
<div class="option-grid">
  {#each cards as card (card.id)}
    <button class="option" onclick={() => onopen(card.id)}>
      <div class="option-top">
        <strong>{card.title}</strong>
        <span class="status" class:off={card.status === 'Off'} class:warn={card.status === 'Inactive'}>{t(card.status)}</span>
      </div>
      <p>{card.text}</p>
    </button>
  {/each}
</div>
