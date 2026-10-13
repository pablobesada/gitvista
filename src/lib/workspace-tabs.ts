import type { RootBadgeState, WorkspaceSnapshot } from './types';

/** Tabs retain paths and scalar badges only; snapshots/diffs belong to the active root. */
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

/** Root state uses every raw repo entry, independent of mode, search or visibility. */
export function workspaceBadge(snapshot: WorkspaceSnapshot, error: string | null, prior?: RootBadgeState): RootBadgeState {
  const changed = snapshot.repositories.some(repo => repo.entries.length > 0);
  const failure = error ?? snapshot.repositories.find(repo => repo.error)?.error ?? null;
  if (changed) return { hasChanges: true, stale: snapshot.scanning || !!failure, error: failure };
  if (snapshot.scanning) return { hasChanges: prior?.hasChanges ?? null, stale: true, error: failure };
  if (failure) return { hasChanges: prior?.hasChanges === true ? true : null, stale: true, error: failure };
  return { hasChanges: false, stale: false, error: null };
}

export function badgeDescription(badge: RootBadgeState | undefined, active: boolean): string {
  if (!badge || badge.hasChanges === null) return `Cambios sin verificar${badge?.error ? ': ' + badge.error : ''}`;
  const text = badge.hasChanges ? 'Con cambios' : 'Sin cambios';
  return `${text}${badge.stale ? ' · revisión pendiente' : ''}${!active ? ' · última revisión; se actualiza al volver o con ⌘R' : ''}${badge.error ? ': ' + badge.error : ''}`;
}
