/** Tabs retain identities only; Git snapshots and diffs belong to the active root. */
export function restoreRoots(openedRoots: readonly string[], lastRoot: string | null): string[] {
  return [...new Set([...openedRoots, ...(lastRoot ? [lastRoot] : [])].filter(Boolean))];
}

export function canonicalizeTab(roots: readonly string[], requested: string, canonical: string): string[] {
  return [...new Set(roots.map(root => root === requested ? canonical : root))];
}

export function closeTab(roots: readonly string[], active: string | null, removed: string) {
  const index = roots.indexOf(removed);
  const remaining = roots.filter(root => root !== removed);
  return { roots: remaining, active: removed === active ? remaining[Math.min(index, remaining.length - 1)] ?? null : active };
}

export function rootLabel(root: string): string {
  return root.split('/').filter(Boolean).pop() ?? '/';
}
