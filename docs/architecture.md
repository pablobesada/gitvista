# Architecture

GitVista has one active workspace and one active diff. Its application-independent core contains serialized models, native paths, cancellation and a byte-budgeted cache. The desktop shell composes those with a bounded Git CLI adapter and a shared watcher service.

## Data flow

1. `open_workspace(root, Channel)` canonicalizes the root, cancels the previous workspace, installs the channel and returns an empty scanning snapshot.
2. Discovery validates `.git` files/directories and resolves each worktree's Git directory and common directory. It does not use a parent's `.gitignore` to exclude nested repositories.
3. Status tasks publish individual `repository` updates. Snapshots are authoritative for topology; `done` never means “only keep repositories updated in this scan”, because watcher scans can be partial.
4. The UI merges entries by `relativePathKey`, a component-preserving encoding of native path bytes, and uses stable opaque entry IDs for Git requests. Display strings are labels, not identities: different non-UTF-8 names may have identical labels. Original paths remain in Rust; display strings must never be converted back into filesystem arguments.
5. Selecting a file cancels the previous foreground query. `get_diff` returns a Git patch with three context lines and a generation/request ID. Svelte rejects stale responses and mounts one CodeView item.
6. Expanding a hunk lazily loads the selected file's sides. Pierre hydrates existing patch metadata without recomputing Git's alignment. The contents are not persisted as a live truth.

## Command contract

The frontend has `open_workspace`, `close_workspace`, `refresh_workspace`, `add_repository`, `get_diff`, `load_diff_contents`, `get_preferences` and `save_preferences`. Workspace updates use a Tauri channel rather than broadcast JSON events. Every file request has an opaque `entryId`, comparison `mode` and monotonically increasing `requestId`.

Root tabs persist only an ordered `openedRoots` list and `lastRoot`. Legacy preferences restore their last root as a single tab. Canonical paths deduplicate aliases. Root transitions are serialized at the bridge and frontend session IDs reject obsolete events and diffs. Switching roots cancels the old watchers/jobs and clears snapshots, patches and mounted content; inactive tabs hold no Git state. `close_workspace` also disconnects IPC and invalidates the generation when the last tab closes. Preferences use a shared write lock around atomic replacement to serialize UI saves and native transitions.

Modes are `total` (HEAD → working tree), `unstaged` (index → working tree), and `staged` (HEAD → index). A repository without HEAD has an empty original. Added/deleted sides are null. Conflicts are informational in v0.1.

Tree presentation supports workspace folders and peer repository groups. Both layouts resolve the same comparison-specific native path ownership before applying search or per-repo untracked visibility, so hiding a child repo's untracked file cannot expose a duplicate parent entry. Presentation changes do not rescan Git. Layout and visibility settings are additive preferences with backward-compatible serde defaults; expansion keys are independent per workspace/layout. Untracked visibility is toggled inline at each repository row. Hidden-only repositories retain a header or folder skeleton so their toggle stays reachable. File activation and the visibility action use sibling buttons; toggling the eye never expands a folder or selects a file.

## Invariants

- No frontend filesystem or shell permission. Native Git queries are allowlisted in the adapter.
- A maximum of three background Git jobs and one foreground job. Blocking process work never runs on the UI thread.
- Cancellation kills and reaps processes, including descendant Git processes on Unix. Captured stdout and command runtime are bounded.
- Watchers invalidate; Git remains the source of truth. Error results retain old entries with a stale indicator.
- Read-only queries do not update the index or Git config. Content conversion and external helper limitations must be surfaced.
- Workspace generations guard root switching; per-repository revisions prevent reuse of cached diffs after refresh.
- Rust cached patches have a 32 MiB byte budget. Renderer worker count/cache limits are explicit. Destroy old DOM/models when switching files.
- Settings live in the platform's application configuration directory and are written via atomic replacement. Git state is not stored as confirmed-current status.

## Future changes

History/comparison APIs can extend `GitBackend`; staging should be a separate explicit write path. Additional platforms can reuse the core and adapter but require native watcher/WebView tests. Simultaneously active workspaces and plugins are deliberately absent from the MVP.
