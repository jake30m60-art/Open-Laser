import { setLocale, type Locale } from './index';

const KEY = 'ol-locale';

/** The language chosen on this device; German unless English was picked. */
export function storedLocale(): Locale {
  try { return localStorage.getItem(KEY) === 'en' ? 'en' : 'de'; } catch { return 'de'; }
}

/** Labels are built once while the modules load, so a new language takes a reload. */
export function chooseLocale(value: Locale): void {
  try { localStorage.setItem(KEY, value); } catch { /* The default stays in use. */ }
  location.reload();
}

// Initialize before the shell's modules create their labels.
const locale = storedLocale();
setLocale(locale);
document.documentElement.lang = locale;
