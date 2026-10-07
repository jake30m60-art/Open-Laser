import catalog from './de.json';

export type Locale = 'de' | 'en';
let locale: Locale = 'en';
/** Set before mounting the UI; internal protocol values remain English. */
export function setLocale(value: Locale): void { locale = value; }

/** Only explicitly marked operator text is translated. Parameters are never translated. */
export function t(source: string | null | undefined, ...parameters: unknown[]): string {
  if (source == null) return '';
  const text = locale === 'de' ? (catalog as Record<string, string>)[source] ?? source : source;
  return text.replace(/\{(\d+)\}/g, (token, index: string) => {
    const i = Number(index);
    return i < parameters.length ? String(parameters[i]) : token;
  });
}
