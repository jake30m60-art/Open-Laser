import type { DraftView, Features, Fill, FillRegion } from '../api';
import { setFill } from './drawing-layers';

/** The staged selection is the entire engraving edit, not a change to the saved job. */
export function engravingFeatures(draft: DraftView, selected: readonly number[], regions: readonly FillRegion[], fill: Fill | null): Features {
  const features = structuredClone(draft.features);
  const markLayers = new Set(draft.layers.filter(l => l.mode === 'mark').map(l => l.name));
  const cutting = draft.layers.find(l => l.mode !== 'mark')?.name ?? '0';
  const old = new Set([...(draft.preview?.contours ?? []), ...(draft.preview?.engraving_regions ?? [])]
    .filter(c => markLayers.has(c.layer)).flatMap(c => c.sources));
  const sources = new Set(selected.flatMap(i => draft.placed[i] ? [draft.placed[i]!.source] : []));
  const edits = new Map((features.layer_edits ?? []).map(e => [e.contour, e.layer]));
  for (const i of old) { const source = draft.placed[i]?.source; if (source !== undefined) edits.set(source, cutting); }
  for (const source of sources) edits.set(source, 'Engraving');
  features.layer_edits = [...edits].map(([contour, layer]) => ({ contour, layer }));
  const order = [...new Set([...draft.layers.map(l => l.name), 'Engraving', cutting])];
  features.layers ??= [];
  if (!features.layers.some(l => l.name === 'Engraving')) features.layers.push({ name: 'Engraving', mode: 'mark', color: null, machining: null });
  features.layers.find(l => l.name === 'Engraving')!.mode = 'mark';
  features.skip_layers = features.skip_layers.filter(l => l !== 'Engraving');
  setFill(features, 'Engraving', fill, order);
  features.fill_regions = fill ? regions.flatMap(region => {
    const owner = draft.placed[region.contour];
    if (!owner) return [];
    return draft.placed.flatMap((placed, contour) => {
      if (placed.source !== owner.source) return [];
      const holes = region.holes.flatMap(hole => {
        const source = draft.placed[hole]?.source;
        const match = contour === region.contour ? hole : draft.placed.findIndex(p => p.source === source && p.copy === placed.copy);
        return match < 0 ? [] : [match];
      });
      return [{ contour, holes }];
    });
  }) : [];
  features.fill_regions = [...new Map(features.fill_regions.map(region => [region.contour, region])).values()];
  return features;
}
