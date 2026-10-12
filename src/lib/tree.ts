import type { ChangeEntry, DiffMode, Repository, RepoState, TreeView } from './types';

export interface ChangeNode {
  key: string;
  name: string;
  path: string;
  directory: boolean;
  repository: Repository | null;
  entry: ChangeEntry | null;
  children: ChangeNode[];
  changeCount: number;
  repositoryRoot?: boolean;
}
export interface TreeRow { node: ChangeNode; depth: number; }
const nameOrder = new Intl.Collator(undefined, { numeric: true });

// Native keys encode raw path bytes component by component. UTF-8 fixtures can
// derive the same representation; display strings never identify native paths.
function pathKey(value: { relativePath: string; relativePathKey?: string }): string {
  if (value.relativePathKey !== undefined) return value.relativePathKey;
  const path = value.relativePath.replace(/\/$/, '');
  if (path === '.') return '';
  return path.split('/').filter(Boolean).map(component =>
    encodeURIComponent(component).replace(/[!'()*]/g, character => `%${character.charCodeAt(0).toString(16).toUpperCase()}`),
  ).join('/');
}

export function hasStatus(status: string): boolean {
  return status !== '.' && status !== ' ' && status !== '';
}

/** A prepared deletion with an untracked replacement remains a tracked change. */
export function isUntracked(entry: ChangeEntry): boolean {
  return entry.indexStatus === '?';
}

export interface TreeOptions { view?: TreeView; untrackedByRepo?: Readonly<Record<string, boolean>>; }

export function entryInMode(entry: ChangeEntry, mode: DiffMode): boolean {
  if (entry.indexStatus === 'U' || entry.worktreeStatus === 'U') return true;
  return mode === 'total' || (mode === 'staged' ? hasStatus(entry.indexStatus) && entry.indexStatus !== '?' : hasStatus(entry.worktreeStatus));
}

export function statusLabel(entry: ChangeEntry, mode: DiffMode): string {
  const statuses = mode === 'staged' ? entry.indexStatus : mode === 'unstaged' ? entry.worktreeStatus : entry.indexStatus + entry.worktreeStatus;
  if (statuses.includes('U')) return 'U';
  if (statuses.includes('?')) return '?';
  if (statuses.includes('R')) return 'R';
  if (statuses.includes('D')) return 'D';
  if (statuses.includes('A')) return 'A';
  if (statuses.includes('T')) return 'T';
  return 'M';
}

/** Ownership precedes visibility filters so hidden child files never leak via a parent. */
function entryOwners(states: RepoState[], mode: DiffMode) {
  const nestedPaths = new Set(states.map(state => pathKey(state.repository)));
  // Resolve each comparison's paths to its deepest matching repository,
  // independently of asynchronous status arrival order and search filtering.
  const owners = new Map<string, { entry: ChangeEntry; state: RepoState; depth: number }>();
  for (const state of states) {
    const depth = pathKey(state.repository).split('/').filter(Boolean).length;
    for (const entry of state.entries) {
      if (!entryInMode(entry, mode)) continue;
      // The parent may report an untracked child repository as a directory. Its
      // independently queried changes are the canonical representation.
      const path = pathKey(entry);
      if (entry.kind === 'repository' && nestedPaths.has(path)) continue;
      const prior = owners.get(path);
      if (!prior || depth > prior.depth || (depth === prior.depth && state.repository.id < prior.state.repository.id)) {
        owners.set(path, { entry, state, depth });
      }
    }
  }
  return owners;
}

export function hiddenUntrackedCount(states: RepoState[], mode: DiffMode, visibility: Readonly<Record<string, boolean>>): number {
  if (!Object.values(visibility).includes(false)) return 0;
  let count = 0;
  for (const { entry, state } of entryOwners(states, mode).values()) {
    if (isUntracked(entry) && visibility[state.repository.id] === false) count++;
  }
  return count;
}

/** Builds changed paths either in the workspace hierarchy or under peer repositories. */
export function buildChangeTree(states: RepoState[], mode: DiffMode, query = '', options: TreeOptions = {}): ChangeNode[] {
  const roots: ChangeNode[] = [];
  const nodes = new Map<string, ChangeNode>();
  const groups = new Map<string, ChangeNode>();
  const repoByRelative = new Map(states.map(state => [pathKey(state.repository), state.repository]));
  const tokens = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const separate = options.view === 'repositories';
  const owners = entryOwners(states, mode);
  function groupFor(repository: Repository): ChangeNode {
    let group = groups.get(repository.id);
    if (!group) {
      group = { key: `\0repo:${repository.id}`, path: repository.relativePath || '.', name: repository.path.split('/').filter(Boolean).pop() ?? 'Repositorio', directory: true, repository, repositoryRoot: true, entry: null, children: [], changeCount: 0 };
      groups.set(repository.id, group);
      roots.push(group);
    }
    return group;
  }
  const retainedHiddenRepos = new Set<string>();
  function retainHiddenRepository(repository: Repository) {
    if (retainedHiddenRepos.has(repository.id)) return;
    retainedHiddenRepos.add(repository.id);
    const searchable = `${repository.relativePath} ${repository.path.split('/').pop()}`.toLocaleLowerCase();
    if (!tokens.every(token => searchable.includes(token))) return;
    if (separate) { groupFor(repository); return; }
    const repoKey = pathKey(repository);
    // The workspace header already exposes the root repository's control.
    if (!repoKey) return;
    const parts = repository.relativePath.split('/').filter(Boolean);
    const keyParts = repoKey.split('/');
    let siblings = roots;
    for (let index = 0; index < parts.length; index++) {
      const key = keyParts.slice(0, index + 1).join('/');
      let node = nodes.get(key);
      if (!node) {
        node = { key, path: parts.slice(0, index + 1).join('/'), name: parts[index], directory: true, repository: repoByRelative.get(key) ?? null, entry: null, children: [], changeCount: 0 };
        nodes.set(key, node);
        siblings.push(node);
      }
      node.directory = true;
      siblings = node.children;
    }
  }
  for (const [entryKey, { entry, state }] of owners) {
    if (isUntracked(entry) && options.untrackedByRepo?.[state.repository.id] === false) {
      retainHiddenRepository(state.repository);
      continue;
    }
    if (tokens.length) {
      const searchable = `${entry.relativePath} ${state.repository.relativePath} ${state.repository.path.split('/').pop()}`.toLocaleLowerCase();
      if (!tokens.every(token => searchable.includes(token))) continue;
    }
    const repoKey = pathKey(state.repository);
    const repoPath = state.repository.relativePath.replace(/\/$/, '').replace(/^\.$/, '');
    const localKey = separate && repoKey && entryKey.startsWith(repoKey + '/') ? entryKey.slice(repoKey.length + 1) : entryKey;
    const localPath = separate && repoPath && entry.relativePath.startsWith(repoPath + '/') ? entry.relativePath.slice(repoPath.length + 1) : entry.relativePath;
    const parts = localPath.replace(/\/$/, '').split('/').filter(Boolean);
    const keyParts = localKey.split('/');
    if (!parts.length) continue;
    let siblings = roots;
    let prefix = '';
    if (separate) {
      const group = groupFor(state.repository);
      prefix = group.key + '/';
      siblings = group.children;
    }
    for (let i = 0; i < parts.length; i++) {
      const path = parts.slice(0, i + 1).join('/');
      const key = prefix + keyParts.slice(0, i + 1).join('/');
      let node = nodes.get(key);
      if (!node) {
        node = { key, path, name: parts[i], directory: i < parts.length - 1, repository: separate ? null : repoByRelative.get(key) ?? null, entry: null, children: [], changeCount: 0 };
        nodes.set(key, node);
        siblings.push(node);
      }
      if (i === parts.length - 1) node.entry = entry;
      else node.directory = true;
      siblings = node.children;
    }
  }
  function finish(items: ChangeNode[]): number {
    let count = 0;
    for (const node of items) {
      if (node.children.length) {
        node.directory = true;
        if (node.entry) {
          // A changed gitlink is independent of changes inside its repository.
          // NUL cannot appear in filesystem names, so this UI key cannot collide
          // with an actual child file. Preserve the original backend entry ID.
          const entry = node.entry;
          const name = entry.kind === 'file' ? 'Archivo anterior' : 'Referencia Git';
          node.children.push({ key: `${node.key}/\0git-reference`, path: `${node.path}/${name}`, name, directory: false, repository: null, entry, children: [], changeCount: 1 });
          node.entry = null;
        }
        node.changeCount = finish(node.children);
      } else node.changeCount = node.entry ? 1 : 0;
      count += node.changeCount;
    }
    items.sort((a, b) => Number(b.directory) - Number(a.directory) || nameOrder.compare(a.name, b.name) || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0));
    return count;
  }
  finish(roots);
  if (separate) roots.sort((a, b) => Number(!pathKey(b.repository!)) - Number(!pathKey(a.repository!)) || nameOrder.compare(a.repository!.relativePath, b.repository!.relativePath) || nameOrder.compare(a.key, b.key));
  return roots;
}

export function directoryPaths(nodes: ChangeNode[]): string[] {
  const result: string[] = [];
  const pending = [...nodes].reverse();
  while (pending.length) {
    const node = pending.pop()!;
    if (node.directory) {
      result.push(node.key);
      for (let i = node.children.length - 1; i >= 0; i--) pending.push(node.children[i]);
    }
  }
  return result;
}

export function flattenTree(nodes: ChangeNode[], expanded: ReadonlySet<string>, forceExpand = false, depth = 0): TreeRow[] {
  const rows: TreeRow[] = [];
  const pending = nodes.map(node => ({ node, depth })).reverse();
  while (pending.length) {
    const row = pending.pop()!;
    rows.push(row);
    if (row.node.directory && (forceExpand || expanded.has(row.node.key))) {
      for (let i = row.node.children.length - 1; i >= 0; i--) pending.push({ node: row.node.children[i], depth: row.depth + 1 });
    }
  }
  return rows;
}

export function allEntries(nodes: ChangeNode[]): ChangeEntry[] {
  const entries: ChangeEntry[] = [];
  const pending = [...nodes].reverse();
  while (pending.length) {
    const node = pending.pop()!;
    if (node.entry) entries.push(node.entry);
    else for (let i = node.children.length - 1; i >= 0; i--) pending.push(node.children[i]);
  }
  return entries;
}
