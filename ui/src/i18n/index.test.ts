import { afterEach, describe, expect, it } from 'vitest';
import catalog from './de.json';
import { setLocale, t } from './index';

afterEach(() => setLocale('en'));
describe('operator translations', () => {
  it('translates German labels while preserving unknown text', () => {
    setLocale('de');
    expect(t('Flying cut')).toBe('Flugschneiden');
    expect(t('Part quantity')).toBe('Teileanzahl');
    expect(t('radius')).toBe('radius');
    expect(t('Mein Teil.dxf')).toBe('Mein Teil.dxf');
  });
  it('inserts user parameters literally without treating them as translation keys', () => {
    setLocale('de');
    expect(t('Delete {0}', 'Flying cut')).toBe('Flying cut löschen');
    expect(t('Delete {0}', '$&{1}')).toBe('$&{1} löschen');
    expect(t('Delete {0}')).toBe('{0} löschen');
  });
  it('keeps every translation placeholder, including repeated parameters', () => {
    for (const [source, translated] of Object.entries(catalog)) {
      const placeholders = (s: string) => [...s.matchAll(/\{\d+\}/g)].map(m => m[0]).sort();
      expect(placeholders(translated), source).toEqual(placeholders(source));
      expect(translated.trim(), source).not.toBe('');
      expect(translated, source).not.toContain('\uFFFD');
    }
  });
  it('retains English fallback and parameters', () => {
    setLocale('en');
    expect(t('Flying cut')).toBe('Flying cut');
    expect(t('Delete {0}', 'Test')).toBe('Delete Test');
  });
});
