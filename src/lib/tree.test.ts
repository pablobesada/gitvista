import { describe, expect, it } from 'vitest';
import { allEntries, buildChangeTree, flattenTree, hiddenUntrackedCount, statusLabel } from './tree';
import type { ChangeEntry, RepoState } from './types';
const repo = (path: string, relativePath: string, entries: ChangeEntry[]): RepoState => ({ repository: { id: path, path, relativePath, gitDir: path + '/.git', commonDir: path + '/.git', branch: 'main' }, entries, error: null, elapsedMs: 1 });
const entry = (relativePath: string, overrides: Partial<ChangeEntry> = {}): ChangeEntry => ({ id: relativePath, repoId: '/root', path: relativePath, oldPath: null, absolutePath: '/root/' + relativePath, relativePath, indexStatus: '.', worktreeStatus: 'M', kind: 'file', ...overrides });

describe('changed-path tree', () => {
  it('places independent repositories at the same level with local file paths', () => {
    const parent = entry('src/main.ts', { id: 'parent-file' });
    const child = entry('scriptdirs/src/opencustom/records/Order.es6', { id: 'child-file', repoId: '/root/scriptdirs/src/opencustom' });
    const states = [repo('/root/scriptdirs/src/opencustom', 'scriptdirs/src/opencustom', [child]), repo('/root', '', [parent])];
    const tree = buildChangeTree(states, 'total', '', { view: 'repositories' });
    expect(tree.map(node => node.name)).toEqual(['root', 'opencustom']);
    expect(tree.map(node => node.repository?.id)).toEqual(['/root', '/root/scriptdirs/src/opencustom']);
    expect(tree[1].children[0].path).toBe('records');
    const rows = flattenTree(tree, new Set(tree.map(node => node.key)));
    expect(rows.filter(row => row.node.repositoryRoot).map(row => row.depth)).toEqual([0, 0]);
    expect(allEntries(tree)).toEqual([parent, child]);
    expect(allEntries(buildChangeTree(states, 'total', 'opencustom', { view: 'repositories' }))).toEqual([child]);
  });
  it('keeps equal filenames in distinct repository groups and native keys distinct', () => {
    const a = entry('a�/same.ts', { id: 'a', repoId: 'repo-a', relativePathKey: 'a%FF/same.ts' });
    const b = entry('a�/same.ts', { id: 'b', repoId: 'repo-b', relativePathKey: 'a%FE/same.ts' });
    const first = repo('/root/a�', 'a�', [a]);
    first.repository = { ...first.repository, id: 'repo-a', relativePathKey: 'a%FF' };
    const second = repo('/root/a�', 'a�', [b]);
    second.repository = { ...second.repository, id: 'repo-b', relativePathKey: 'a%FE' };
    const tree = buildChangeTree([first, second], 'total', '', { view: 'repositories' });
    const rows = flattenTree(tree, new Set(), true);
    expect(new Set(rows.map(row => row.node.key)).size).toBe(4);
    expect(allEntries(tree).map(value => value.id).sort()).toEqual(['a', 'b']);
    expect(tree.every(node => node.children[0].name === 'same.ts')).toBe(true);
  });
  it('hides untracked files independently per repo in both layouts without hiding tracked changes', () => {
    const ownNew = entry('new.txt', { id: 'own-new', indexStatus: '?', worktreeStatus: '?' });
    const childNew = entry('child/new.txt', { id: 'child-new', repoId: '/root/child', indexStatus: '?', worktreeStatus: '?' });
    const prepared = entry('child/prepared.txt', { id: 'prepared', repoId: '/root/child', indexStatus: 'A', worktreeStatus: '.' });
    const deletion = entry('child/deleted.txt', { id: 'deletion', repoId: '/root/child', indexStatus: 'D', worktreeStatus: '?' });
    const states = [repo('/root', '', [ownNew]), repo('/root/child', 'child', [childNew, prepared, deletion])];
    for (const view of ['folders', 'repositories'] as const) {
      const options = { view, untrackedByRepo: { '/root/child': false } };
      expect(allEntries(buildChangeTree(states, 'total', '', options)).map(value => value.id).sort()).toEqual(['deletion', 'own-new', 'prepared']);
      expect(allEntries(buildChangeTree(states, 'staged', '', options)).map(value => value.id).sort()).toEqual(['deletion', 'prepared']);
      expect(hiddenUntrackedCount(states, 'total', options.untrackedByRepo)).toBe(1);
      expect(hiddenUntrackedCount(states, 'staged', options.untrackedByRepo)).toBe(0);
      expect(allEntries(buildChangeTree(states, 'total', '', { view, untrackedByRepo: { '/root/child': true } }))).toHaveLength(4);
    }
  });
  it('never resurfaces a hidden child entry through its parent and reports hidden-only results', () => {
    const parent = entry('child/new.txt', { id: 'parent', indexStatus: '?', worktreeStatus: '?' });
    const child = entry('child/new.txt', { id: 'child', repoId: '/root/child', indexStatus: '?', worktreeStatus: '?' });
    const states = [repo('/root', '', [parent]), repo('/root/child', 'child', [child])];
    const options = { view: 'repositories' as const, untrackedByRepo: { '/root/child': false } };
    const tree = buildChangeTree(states, 'total', '', options);
    expect(tree).toHaveLength(1);
    expect(tree[0].repository?.id).toBe('/root/child');
    expect(tree[0].changeCount).toBe(0);
    expect(allEntries(tree)).toEqual([]);
    expect(hiddenUntrackedCount(states, 'total', options.untrackedByRepo)).toBe(1);
  });
  it('retains a hidden-only nested repo control in the folder layout and scopes search to its repo', () => {
    const item = entry('plugins/tool/untracked.txt', { repoId: '/root/plugins/tool', indexStatus: '?', worktreeStatus: '?' });
    const states = [repo('/root/plugins/tool', 'plugins/tool', [item])];
    const options = { untrackedByRepo: { '/root/plugins/tool': false } };
    const tree = buildChangeTree(states, 'total', '', options);
    expect(tree[0].name).toBe('plugins');
    expect(tree[0].children[0].repository?.id).toBe('/root/plugins/tool');
    expect(allEntries(tree)).toEqual([]);
    expect(allEntries(buildChangeTree(states, 'total', '', { untrackedByRepo: { '/root/plugins/tool': true } }))).toEqual([item]);
    expect(buildChangeTree(states, 'total', 'tool', options)).toHaveLength(1);
    expect(buildChangeTree(states, 'total', 'untracked.txt', options)).toEqual([]);
  });
  it('preserves a single hierarchy and marks nested repository boundaries', () => {
    const tree = buildChangeTree([repo('/root', '', [entry('src/main.ts'), entry('plugins/tool', { kind: 'repository' })]), repo('/root/plugins/tool', 'plugins/tool', [entry('plugins/tool/src/lib.rs', { repoId: '/root/plugins/tool' })])], 'total');
    expect(tree.map(n => n.name)).toEqual(['plugins', 'src']);
    expect(tree[0].children[0].repository?.relativePath).toBe('plugins/tool');
    expect(allEntries(tree).map(e => e.relativePath)).toEqual(['plugins/tool/src/lib.rs', 'src/main.ts']);
  });
  it('retains changed submodule references alongside dirty child files', () => {
    const gitlink = entry('plugins/tool', { id: 'parent:plugins/tool', kind: 'submodule', indexStatus: 'M', worktreeStatus: '.' });
    const childFile = entry('plugins/tool/src/lib.rs', { id: 'child:src/lib.rs', repoId: '/root/plugins/tool', path: 'src/lib.rs' });
    const tree = buildChangeTree([repo('/root', '', [gitlink]), repo('/root/plugins/tool', 'plugins/tool', [childFile])], 'total');
    const folder = tree[0].children[0];
    expect(folder.directory).toBe(true);
    expect(folder.entry).toBeNull();
    expect(folder.changeCount).toBe(2);
    expect(folder.children.find(node => node.name === 'Referencia Git')?.entry).toBe(gitlink);
    const rows = flattenTree(tree, new Set(['plugins', 'plugins/tool', 'plugins/tool/src']));
    expect(rows.filter(row => row.node.entry).map(row => row.node.entry?.id).sort()).toEqual([gitlink.id, childFile.id].sort());
    expect(new Set(rows.map(row => row.node.key)).size).toBe(rows.length);
  });
  it('chooses the deepest repository for overlapping tracked paths in either arrival order', () => {
    const parentFile = entry('plugins/tool/src/lib�.rs', { relativePathKey: 'plugins/tool/src/lib%FF.rs', id: 'parent:plugins/tool/src/lib%FF.rs', indexStatus: 'M', worktreeStatus: '.' });
    const childFile = entry('plugins/tool/src/lib�.rs', { relativePathKey: 'plugins/tool/src/lib%FF.rs', id: 'child:src/lib%FF.rs', repoId: '/root/plugins/tool', path: 'src/lib�.rs' });
    const states = [repo('/root', '', [parentFile]), repo('/root/plugins/tool', 'plugins/tool', [childFile])];
    for (const order of [states, [...states].reverse()]) {
      expect(allEntries(buildChangeTree(order, 'total'))).toEqual([childFile]);
      expect(allEntries(buildChangeTree(order, 'unstaged'))).toEqual([childFile]);
      // Each comparison remains reviewable when the child's change is unstaged
      // but the parent independently contains a prepared change at this path.
      expect(allEntries(buildChangeTree(order, 'staged'))).toEqual([parentFile]);
    }
  });
  it('retains different native ancestor and leaf keys with identical display names', () => {
    const folder = 'packages/tool�';
    const display = `${folder}/src/file�.rs`;
    const keys = ['packages/tool%FE', 'packages/tool%FF'];
    const children = keys.map((key, index) => {
      const state = repo(`/root/${folder}`, folder, [entry(display, { id: `child-${index}:file`, repoId: `child-${index}`, path: 'src/file�.rs', relativePathKey: `${key}/src/file%FF.rs` })]);
      state.repository.id = `child-${index}`;
      state.repository.relativePathKey = key;
      return state;
    });
    const parentMetadata = keys.map((key, index) => entry(folder, { id: `parent-${index}:folder`, relativePathKey: key, kind: 'repository' }));
    const tree = buildChangeTree([repo('/root', '', parentMetadata), ...children], 'total');
    const rows = flattenTree(tree, new Set(), true);
    const folders = rows.filter(row => row.node.repository);
    expect(folders.map(row => row.node.name)).toEqual(['tool�', 'tool�']);
    expect(folders.map(row => row.node.repository?.id).sort()).toEqual(['child-0', 'child-1']);
    expect(allEntries(tree).map(change => change.id).sort()).toEqual(['child-0:file', 'child-1:file']);
    expect(rows.filter(row => row.node.entry).map(row => row.node.name)).toEqual(['file�.rs', 'file�.rs']);
    expect(new Set(rows.map(row => row.node.key)).size).toBe(rows.length);
    expect(flattenTree(tree, new Set(['packages', keys[0], `${keys[0]}/src`])).filter(row => row.node.entry).map(row => row.node.entry?.id)).toEqual(['child-0:file']);
  });
  it('filters by comparison and search without showing unrelated ancestors', () => {
    const states = [repo('/root', '', [entry('src/a.ts', { indexStatus: 'A', worktreeStatus: '.' }), entry('docs/b.md')])];
    expect(allEntries(buildChangeTree(states, 'staged')).map(e => e.path)).toEqual(['src/a.ts']);
    expect(allEntries(buildChangeTree(states, 'unstaged')).map(e => e.path)).toEqual(['docs/b.md']);
    expect(allEntries(buildChangeTree(states, 'total', 'docs b.md')).map(e => e.path)).toEqual(['docs/b.md']);
  });
  it('only flattens open directories and retains Unicode paths', () => {
    const tree = buildChangeTree([repo('/root', '', [entry('café/a.ts'), entry('z.ts')])], 'total');
    expect(flattenTree(tree, new Set()).map(r => r.node.path)).toEqual(['café', 'z.ts']);
    expect(flattenTree(tree, new Set(['caf%C3%A9'])).map(r => [r.node.path, r.depth])).toEqual([['café', 0], ['café/a.ts', 1], ['z.ts', 0]]);
  });
  it('flattens 100,000 siblings without oversized spread calls', () => {
    const changes = Array.from({ length: 100_000 }, (_, i) => entry(`src/file${i}.ts`));
    const tree = buildChangeTree([repo('/root', '', changes)], 'total');
    const rows = flattenTree(tree, new Set(['src']));
    expect(rows).toHaveLength(100_001);
    expect(rows[1].node.name).toBe('file0.ts');
    expect(rows.at(-1)?.node.name).toBe('file99999.ts');
  });
  it('shows conflict and untracked status distinctly', () => {
    expect(statusLabel(entry('x', { indexStatus: 'U', worktreeStatus: 'U' }), 'total')).toBe('U');
    expect(statusLabel(entry('x', { worktreeStatus: '?' }), 'unstaged')).toBe('?');
  });
});
