import type { ImageFill, Preview } from '../api';

/** What an image layer engraves with until changed; the server's defaults. */
export const defaultImage = (): ImageFill => ({ mode: 'grayscale', min_power: 10, max_power: 100, invert: false, gamma: 1 });

/** The layers whose shapes frame a picture. */
export function imageLayers(preview: Preview | null | undefined): Set<string> {
  const layers = new Set<string>();
  for (const raster of preview?.rasters ?? []) {
    const layer = preview!.contours.find((c) => c.sources.includes(raster.contour))?.layer;
    if (layer) layers.add(layer);
  }
  return layers;
}
