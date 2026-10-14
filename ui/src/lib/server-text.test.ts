import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { setLocale } from '../i18n/index';
import { plain, serverText } from './plain';

describe('server messages in German', () => {
  beforeAll(() => setLocale('de'));
  afterAll(() => setLocale('en'));

  it('translates a known message', () => {
    expect(serverText('a move is active')).toBe('Eine Bewegung läuft noch');
  });
  it('translates a prefix and the message after it', () => {
    expect(serverText('Pendant: a move is active')).toBe('Handrad: Eine Bewegung läuft noch');
  });
  it('fills a message with values', () => {
    expect(serverText('the hold for Start must be 0.5 to 5 seconds')).toBe('Die Haltezeit für Starten muss 0.5 bis 5 Sekunden sein');
    expect(serverText('Calibration failed: no machine feedback')).toBe('Kalibrierung fehlgeschlagen: Keine Rückmeldung von der Maschine');
  });
  it('translates clause by clause', () => {
    expect(serverText('Connected; the machine configuration changed; compile again')).toContain('Verbunden');
  });
  it('leaves unknown words as they are', () => {
    expect(serverText('qwerty zzz')).toBe('qwerty zzz');
  });
  it('gives the toast a German sentence', () => {
    expect(plain('Pendant: a move is active').text).toBe('Handrad: Eine Bewegung läuft noch.');
  });
});
