// Turning a picture file into the gray pixels an image part engraves. The
// browser decodes the file; this only weighs its colours and picks the size.

/** The most pixels on an image's long side the server takes. */
export const MAX_SIDE = 4000;
/** Finer than this a laser line cannot resolve, in millimetres per pixel. */
export const FINEST = 0.05;

/** Gray levels from RGBA pixels: perceived brightness, transparency as white. */
export function grayPixels(rgba: ArrayLike<number>): Uint8Array {
  const out = new Uint8Array(Math.floor(rgba.length / 4));
  for (let i = 0; i < out.length; i++) {
    const r = rgba[i * 4]!, g = rgba[i * 4 + 1]!, b = rgba[i * 4 + 2]!, a = rgba[i * 4 + 3]! / 255;
    const luma = 0.299 * r + 0.587 * g + 0.114 * b;
    out[i] = Math.round(luma * a + 255 * (1 - a));
  }
  return out;
}

/** How many pixels to keep: never more than the file has, than the laser
 * can resolve at `widthMm`, or than the server takes. */
export function targetSize(width: number, height: number, widthMm: number): { width: number; height: number } {
  const resolvable = Math.max(1, Math.ceil(widthMm / FINEST));
  const scale = Math.min(1, resolvable / width, MAX_SIDE / Math.max(width, height));
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) };
}

/** The height that keeps the picture's proportions at `widthMm`. */
export const heightFor = (width: number, height: number, widthMm: number): number => (widthMm * height) / width;
