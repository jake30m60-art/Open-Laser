import { expect, it } from 'vitest';
import catalog from './de.json';

// Every fixed text handed to t() or ui.say() has its German, so the German
// screen never shows English by accident and the English one never German.
const files = import.meta.glob(['../**/*.ts', '../**/*.svelte', '!../**/*.test.ts', '!../api/**', '!./**'], { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

it('every fixed text has a German translation', () => {
  expect(Object.keys(files).length).toBeGreaterThan(200);
  const missing: string[] = [];
  const call = /\b(?:t|tr|tl|say)\(\s*(["'])((?:\\.|(?!\1).)*)\1/g;
  for (const [file, text] of Object.entries(files)) {
    for (const m of text.matchAll(call)) {
      const key = m[2]!.replace(/\\(.)/g, '$1');
      if (/[A-Za-z]{2}/.test(key) && (catalog as Record<string, string>)[key] === undefined) missing.push(`${file}: ${key}`);
    }
  }
  expect(missing).toEqual([]);
});

it('no German text is used as a key', () => {
  const german = Object.keys(catalog).filter((key) => /[äöüÄÖÜß]/.test(key));
  expect(german).toEqual([]);
});
