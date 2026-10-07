import { setLocale } from './index';

// Initialize before the shell's modules create their labels.
setLocale('de');
document.documentElement.lang = 'de';
