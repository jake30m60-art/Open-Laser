/** Preparation reports placed contour IDs counted from zero. */
export function issueSources(message: string | null | undefined): Set<number> {
  const ids = new Set<number>();
  for (const match of (message ?? '').matchAll(/contours? (\d+)(?: and (\d+))?/gi)) {
    ids.add(Number(match[1])); if (match[2]) ids.add(Number(match[2]));
  }
  return ids;
}

export function excludedSources(message: string | null | undefined, only: readonly number[] | null | undefined, sources: readonly number[]): Set<number> {
  const excluded=issueSources(message);
  if(only) for(const source of sources) if(!only.includes(source)) excluded.add(source);
  return excluded;
}
