/** Source descriptions cannot prove which gas is physically connected. */
export function airRecipeUsesNitrogen(recipe: { note: string; file_name: string | null }, values: Record<string, string>): boolean {
  return values.CutGasType === '5' && /(?:^|[^a-z])air(?:[^a-z]|$)/i.test(`${recipe.note} ${recipe.file_name ?? ''}`);
}
