export type DiffMode = 'total' | 'unstaged' | 'staged';
export interface Repository { id: string; path: string; relativePath: string; relativePathKey?: string; gitDir: string; commonDir: string; branch: string; }
export interface ChangeEntry { id: string; repoId: string; path: string; oldPath: string | null; absolutePath: string; relativePath: string; relativePathKey?: string; indexStatus: string; worktreeStatus: string; kind: 'file' | 'repository' | 'submodule'; }
export interface RepoState { repository: Repository; entries: ChangeEntry[]; error: string | null; elapsedMs: number; }
export interface WorkspaceSnapshot { root: string; generation: number; repositories: RepoState[]; scanning: boolean; scannedCount: number; }
export interface RootBadge { root: string; hasChanges: boolean | null; error: string | null; }
export interface RootBadgeState { hasChanges: boolean | null; stale: boolean; error: string | null; }
export type WorkspaceEvent = { type: 'reset'; snapshot: WorkspaceSnapshot } | { type: 'repository'; generation: number; state: RepoState; scannedCount: number; totalCount: number } | { type: 'done'; generation: number; elapsedMs: number } | { type: 'error'; generation: number; message: string };
export interface DiffResult { entryId: string; repoId: string; mode: DiffMode; patch: string; kind: 'text' | 'binary' | 'metadata' | 'conflict' | 'empty' | 'tooLarge'; message: string | null; oldPath: string | null; newPath: string; }
export interface DiffResponse { requestId: number; generation: number; result: DiffResult; }
export interface FileContents { name: string; contents: string; cacheKey: string; }
export interface EditVersion { generation: number; token: string; }
export interface DiffContents { oldFile: FileContents | null; newFile: FileContents | null; editVersion?: EditVersion | null; editUnavailableReason?: string | null; }
export type TreeView = 'folders' | 'repositories';
export interface Preferences { recentRoots: string[]; openedRoots: string[]; extraRepositories: Record<string, string[]>; sidebarWidth: number; expandedPaths: Record<string, string[]>; lastRoot: string | null; treeView: TreeView; untrackedByRepo: Record<string, boolean>; }
