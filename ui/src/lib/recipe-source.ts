/** Source descriptions cannot prove which gas is physically connected. */
export function airRecipeUsesNitrogen(recipe: { note: string; file_name: string | null }, values: Record<string, string>): boolean {
  return values.CutGasType === '5' && /(?:^|[^a-z])air(?:[^a-z]|$)/i.test(`${recipe.note} ${recipe.file_name ?? ''}`);
}

/** Where a recipe came from: the machine maker's parameters, OpenLaser's
 *  bundled sample library, or the operator's own. */
export type RecipeOrigin = 'gweike' | 'sample' | 'own';
export function recipeOrigin(recipe: { name: string; note: string }): RecipeOrigin {
  if (/\(Gweike\)/.test(recipe.name) || /Autor: Gweike/.test(recipe.note)) return 'gweike';
  if (/^MATERIAL: .+\nTHICKNESS: /.test(recipe.note)) return 'sample';
  return 'own';
}
