import { describe, expect, it } from 'vitest';
import { badgeDescription, canonicalizeTab, closeTab, restoreRoots, workspaceBadge } from './workspace-tabs';
import { demoWorkspace } from './demo';

describe('workspace tabs', () => {
  it('migrates the previous root and includes a newly opened CLI root', () => {
    expect(restoreRoots([], '/old')).toEqual(['/old']);
    expect(restoreRoots(['/a', '/b', '/a'], '/cli')).toEqual(['/a', '/b', '/cli']);
    expect(restoreRoots([], null)).toEqual([]);
  });
  it('merges canonical aliases without changing tab order', () => {
    expect(canonicalizeTab(['/alias', '/b', '/real'], '/alias', '/real')).toEqual(['/real', '/b']);
  });
  it('closing an inactive root preserves the active root', () => {
    expect(closeTab(['/a', '/b', '/c'], '/a', '/b')).toEqual({ roots: ['/a', '/c'], active: '/a' });
  });
  it('closing the active root chooses its right neighbor, then its left neighbor', () => {
    expect(closeTab(['/a', '/b', '/c'], '/b', '/b').active).toBe('/c');
    expect(closeTab(['/a', '/b', '/c'], '/c', '/c').active).toBe('/b');
    expect(closeTab(['/a'], '/a', '/a')).toEqual({ roots: [], active: null });
  });
  it('marks the root using nested raw entries even when changes are staged, hidden or net-empty', () => {
    const snapshot = demoWorkspace('/fixture', 1);
    snapshot.repositories = snapshot.repositories.filter(repo => repo.repository.id !== '.');
    expect(workspaceBadge(snapshot, null)).toEqual({ hasChanges: true, stale: false, error: null });
    snapshot.repositories[0].entries = snapshot.repositories[0].entries.map(entry => ({ ...entry, indexStatus: 'M', worktreeStatus: 'M' }));
    expect(workspaceBadge(snapshot, null).hasChanges).toBe(true);
  });
  it('never replaces a known change with clean during scanning or errors', () => {
    const snapshot = demoWorkspace('/fixture', 1);
    snapshot.repositories.forEach(repo => repo.entries = []);
    const prior = { hasChanges: true, stale: false, error: null };
    snapshot.scanning = true;
    expect(workspaceBadge(snapshot, null, prior)).toEqual({ ...prior, stale: true });
    snapshot.scanning = false;
    expect(workspaceBadge(snapshot, 'unavailable', prior)).toEqual({ hasChanges: true, stale: true, error: 'unavailable' });
    expect(workspaceBadge(snapshot, 'unavailable').hasChanges).toBeNull();
    expect(workspaceBadge(snapshot, null, prior).hasChanges).toBe(false);
  });
  it('labels unknown and inactive states without claiming they are live', () => {
    expect(badgeDescription(undefined, false)).toContain('sin verificar');
    expect(badgeDescription({ hasChanges: true, stale: false, error: null }, false)).toContain('última revisión');
  });
});
