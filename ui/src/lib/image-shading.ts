import type { ImageFill } from '../api';

/** How dark a gray level (0 black to 1 white) engraves, 0 to 1, after
 *  inversion and gamma: as the server shades it. */
export function darkness(gray: number, image: ImageFill): number {
  const dark = image.invert ? gray : 1 - gray;
  return Math.min(1, Math.max(0, dark)) ** image.gamma;
}

/** The power in percent each cell of a `width` × `height` grid of gray
 *  levels (0 to 255, row by row) is engraved with, by the server's rules:
 *  grayscale between the least and the most, nothing under 1 % dark;
 *  black and white at middle gray; dithering by error diffusion along the
 *  rows. Along the real scan lines the dither runs at the fill's angle, so
 *  its dots fall slightly differently there. */
export function shadeCells(gray: ArrayLike<number>, width: number, height: number, image: ImageFill): Uint8Array {
  const levels = new Uint8Array(width * height);
  const percent = (v: number) => Math.round(Math.min(100, Math.max(0, v)));
  let carry = new Float64Array(width + 2);
  for (let y = 0; y < height; y++) {
    const next = new Float64Array(width + 2);
    for (let x = 0; x < width; x++) {
      const dark = darkness(gray[y * width + x]! / 255, image);
      let level = 0;
      if (image.mode === 'grayscale') level = dark < 0.01 ? 0 : percent(image.min_power + (image.max_power - image.min_power) * dark);
      else if (image.mode === 'threshold') level = dark >= 0.5 ? percent(image.max_power) : 0;
      else {
        const value = dark + carry[x + 1]!;
        const on = value >= 0.5;
        const error = value - (on ? 1 : 0);
        carry[x + 2]! += (error * 7) / 16;
        next[x]! += (error * 3) / 16;
        next[x + 1]! += (error * 5) / 16;
        next[x + 2]! += error / 16;
        level = on ? percent(image.max_power) : 0;
      }
      levels[y * width + x] = level;
    }
    carry = next;
  }
  return levels;
}

/** The grid a picture `widthMm` × `heightMm` engraves on at `spacing`, at
 *  most `limit` cells along its longer side for the screen. */
export function cellGrid(widthMm: number, heightMm: number, spacing: number, limit = 1200): { width: number; height: number } {
  const scale = Math.min(1, limit / (Math.max(widthMm, heightMm) / spacing));
  return {
    width: Math.max(1, Math.round((widthMm / spacing) * scale)),
    height: Math.max(1, Math.round((heightMm / spacing) * scale)),
  };
}

const cache = new Map<string, Promise<string>>();

/** The picture at `url` as it engraves: each cell as dark as its power,
 *  black at 100 %. A data URL, kept per picture and settings. */
export function shadedPicture(url: string, image: ImageFill, grid: { width: number; height: number }): Promise<string> {
  const key = JSON.stringify([url, image, grid]);
  let made = cache.get(key);
  if (!made) {
    made = new Promise<string>((resolve, reject) => {
      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement('canvas');
        canvas.width = grid.width;
        canvas.height = grid.height;
        const ctx = canvas.getContext('2d', { willReadFrequently: true });
        if (!ctx) return reject(new Error('no canvas'));
        ctx.fillStyle = '#fff';
        ctx.fillRect(0, 0, grid.width, grid.height);
        ctx.drawImage(img, 0, 0, grid.width, grid.height);
        const data = ctx.getImageData(0, 0, grid.width, grid.height);
        const gray = new Uint8Array(grid.width * grid.height);
        for (let i = 0; i < gray.length; i++) {
          const [r, g, b] = [data.data[i * 4]!, data.data[i * 4 + 1]!, data.data[i * 4 + 2]!];
          gray[i] = Math.round(0.299 * r + 0.587 * g + 0.114 * b);
        }
        const levels = shadeCells(gray, grid.width, grid.height, image);
        for (let i = 0; i < levels.length; i++) {
          const v = Math.round(255 * (1 - levels[i]! / 100));
          data.data[i * 4] = data.data[i * 4 + 1] = data.data[i * 4 + 2] = v;
          data.data[i * 4 + 3] = 255;
        }
        ctx.putImageData(data, 0, 0);
        resolve(canvas.toDataURL('image/png'));
      };
      img.onerror = () => reject(new Error('the picture did not load'));
      img.src = url;
    });
    made.catch(() => cache.delete(key));
    if (cache.size > 24) cache.delete(cache.keys().next().value!);
    cache.set(key, made);
  }
  return made;
}
