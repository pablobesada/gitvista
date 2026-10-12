import { describe, expect, it } from 'vitest';
import { canonicalizeTab, closeTab, restoreRoots } from './workspace-tabs';

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
});
