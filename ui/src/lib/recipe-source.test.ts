import { expect, it } from 'vitest';
import { airRecipeUsesNitrogen } from './recipe-source';
it('flags source air versus the selected nitrogen valve without changing values', () => {
  const source = { note: 'Autor: Gweike · Datei 1200W_air/Steel.xml', file_name: 'Steel.xml' };
  expect(airRecipeUsesNitrogen(source, { CutGasType: '5' })).toBe(true);
  expect(airRecipeUsesNitrogen(source, { CutGasType: '3' })).toBe(false);
  expect(airRecipeUsesNitrogen({note: '', file_name: 'SS 1mm air.xml'}, {CutGasType:'5'})).toBe(true);
  expect(airRecipeUsesNitrogen({note: 'repair', file_name: 'SS N2.xml'}, {CutGasType:'5'})).toBe(false);
});

it('tells the maker\'s, sample and own recipes apart', async () => {
  const { recipeOrigin } = await import('./recipe-source');
  expect(recipeOrigin({ name: 'Brass (Gweike)', note: '' })).toBe('gweike');
  expect(recipeOrigin({ name: 'Brass', note: 'MATERIAL: Brass\nTHICKNESS: 1 mm\nPROCESS: CUTTING' })).toBe('sample');
  expect(recipeOrigin({ name: 'Brass', note: 'mein Test' })).toBe('own');
});
