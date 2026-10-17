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

The frontend has workspace commands (`open_workspace`, `close_workspace`, `refresh_workspace`, `add_repository`, `get_root_badges`), selected-file commands (`get_diff`, `load_diff_contents`, `save_working_file`), explicit cleanup commands (`preview_repository_cleanup`, `stash_repository`), and application commands (`set_editor_active`, `exit_application`, `get_preferences`, `save_preferences`). Workspace updates use a Tauri channel rather than broadcast JSON events. Every file request has an opaque `entryId`, comparison `mode` and monotonically increasing `requestId`.

Root tabs persist only an ordered `openedRoots` list and `lastRoot`. Legacy preferences restore their last root as a single tab. Canonical paths deduplicate aliases. Root transitions are serialized at the bridge and frontend session IDs reject obsolete events and diffs. Switching roots cancels the old watchers/jobs and clears snapshots, patches and mounted content; inactive tabs retain only cached scalar change badges, never repository snapshots, watchers or diffs. `close_workspace` also disconnects IPC and invalidates the generation when the last tab closes. Preferences use a shared write lock around atomic replacement to serialize UI saves and native transitions.

Modes are `total` (HEAD → working tree), `unstaged` (index → working tree), and `staged` (HEAD → index). A repository without HEAD has an empty original. Added/deleted sides are null. Conflicts are informational in v0.1.

Tree presentation supports workspace folders and peer repository groups. Both layouts resolve the same comparison-specific native path ownership before applying search or per-repo untracked visibility, so hiding a child repo's untracked file cannot expose a duplicate parent entry. Presentation changes do not rescan Git. Layout and visibility settings are additive preferences with backward-compatible serde defaults; expansion keys are independent per workspace/layout. Untracked visibility is toggled inline at each repository row. Hidden-only repositories retain a header or folder skeleton so their toggle stays reachable. File activation and the visibility action use sibling buttons; toggling the eye never expands a folder or selects a file.

## Full-file review and explicit editing

`DiffView` controls changes/full scope and a lazy-loaded Pierre editor. CodeView owns hydration of mounted partial models and associated AST/layout caches. Complete models are constructed separately for additions, deletions and net-empty/metadata comparisons. Editing affects only the new side, retains one mounted item, one highlighter worker and a 50-entry history. No editor state is keyed or retained for inactive files. Dirty drafts remain mounted through status updates; navigation, root/comparison changes and exit go through a discard guard. Native close/exit requests are deferred while an editor is active.

Identical versions have an explicit unchanged region so CodeView's hunk-based virtualizer gives them a nonzero height. Empty documents get a blank editable row while their underlying bytes remain empty. Content loading temporarily defers selected-file refreshes; a stale status revision gets one native retry under the same request identity without replacing the in-flight model. macOS's Quit accelerator uses a custom menu item to enter the same discard guard. Change navigation and the review copy button are suspended during editing; the editor owns ordinary navigation and ⌘C.

Block restoration uses consecutive `change` segments rather than entire context-bearing Git hunks. `diff-actions.ts` converts partial or complete coordinates into bounded draft edits; surrounding text and line endings remain intact. `DiffView` derives live block coordinates from the editor's public retained session and rejects clicks from older revisions/document versions. Render and scroll callbacks reconcile a small overlay containing only visible block buttons; no per-line annotation or full-file DOM is added. Each restoration enters the existing editor and uses a separate undo boundary. Whole-file restoration is also a single draft edit. Both operations retain the native disk-version check and require explicit saving; they add no Git mutations or filesystem commands.

Full content responses can carry an `EditVersion` for the current regular working file. The native token hashes bytes plus file identity/mtime/mode; it is paired with workspace generation. `save_working_file` resolves the opaque entry ID from the active native snapshot, holds root ownership through the bounded write, rechecks the disk token and writes via a temporary file in the same directory. Unix directory descriptors, `openat` with `O_NOFOLLOW`, and `renameat` prevent traversal through file/parent symlinks. Relative traversal and `.git` components are rejected. The query adapter remains read-only; the confirmed repository stash action has a separate write boundary. Saving clears patch cache and schedules a selective status scan.

The disk checks are optimistic concurrency control; another process writing in the narrow interval between the final check and rename cannot be atomically excluded by a generic filesystem API. Atomic replacement preserves permission mode, but does not promise inode identity or all extended metadata. Native tests cover stale versions, root changes, limits, path guards, CRLF preservation, executable mode, watcher reconciliation and unchanged Git index/config bytes.

## Invariants

- No frontend filesystem or shell permission. Native Git queries are allowlisted in the adapter.
- A maximum of three background Git jobs and one foreground job. Blocking process work never runs on the UI thread.
- Cancellation kills and reaps processes, including descendant Git processes on Unix. Captured stdout and command runtime are bounded.
- Watchers invalidate; Git remains the source of truth. Error results retain old entries with a stale indicator.
- Read-only queries do not update the index or Git config. Content conversion and external helper limitations must be surfaced.
- Git commands override `diff.autoRefreshIndex=false` as well as disabling optional locks: [porcelain diff can otherwise refresh cached stat entries](https://git-scm.com/docs/git-diff#Documentation/git-diff.txt-diffautoRefreshIndex) when restored contents match the index. The override is command-local, never persisted to repository configuration.
- Workspace generations guard root switching; per-repository revisions prevent reuse of cached diffs after refresh.
- Rust cached patches have a 32 MiB byte budget. Renderer worker count/cache limits are explicit. Destroy old DOM/models when switching files.
- Settings live in the platform's application configuration directory and are written via atomic replacement. Git state is not stored as confirmed-current status.

## Future changes

History/comparison APIs can extend `GitBackend`; staging should be a separate explicit write path. Additional platforms can reuse the core and adapter but require native watcher/WebView tests. Simultaneously active workspaces and plugins are deliberately absent from the MVP.

## Confirmed repository cleanup

`preview_repository_cleanup(repoId, generation)` resolves the native repository from the active snapshot and returns whole-repository counts plus a fingerprint of HEAD, index and changed native paths/stamps. `stash_repository(repoId, generation, token, includeUntracked)` revalidates identity, generation, disk state and Git-operation/filter preconditions before entering the sole Git mutation boundary. Queries remain read-only. No display path becomes a Git argument.

The write owns the foreground permit and a native exit/root-switch guard through the blocking task, even if its IPC future is abandoned. It starts only after an active status scan completes; watcher scans are suppressed during the write. It uses a private cancellation token and a 120-second deadline, disables recursive submodule operations/hooks/automatic maintenance for this command only, and never invokes a destructive fallback. The result preserves the unique named backup ID even on partial failure, independently verifies remaining changes, invalidates patches and schedules a selected-repo status scan. A failed verification is surfaced as a warning, never a clean result. External Git clients can still race the operation; the backup subject is checked before attributing a changed stash ref to GitVista.

The UI provides a focusable repository context menu and modal confirmation, includes untracked files by explicit checkbox, preserves ignored/nested working trees, and routes active drafts through the existing discard guard. The result includes a copyable recovery command using the immutable stash object ID, to run from the selected repository; frontend display paths never become command arguments. It does not introduce stash history or automatic recovery writes.

## Resource budgets

- Three background Git tasks plus one foreground diff task; the foreground queue cancels stale selections.
- Only the selected diff is mounted. Both the tree and code viewport are virtualized.
- Git metadata is refreshed on filesystem events with a 200 ms debounce. Focus/manual refresh reconcile state; watcher errors are visible and do not turn a repository into “clean”.
- Rust diff cache: 32 MiB, with byte-based eviction. The highlighter uses one worker and two entries per AST cache; large or minified content falls back to plain code.
- Inline changes remain visible beyond the 2,000-character syntax limit. Long lines trim identical ends before character comparison, with a 32,768-character combined middle, 256-edit and 20 ms refinement budget. If refinement exceeds a budget, emphasis covers the differing middle while leaving the identical ends unmarked. The pinned Pierre adapter applies in the worker and local/editor renderer and fails explicitly if its integration point changes.
- Full context is loaded only for the selected file, up to 10 MiB per side. Patches, command time and stdout are bounded.
- Binary files, merge conflicts, external content filters and unsupported encodings have explicit informational states.
- Git commands use direct arguments, NUL-delimited status and original native paths. They disable optional index locks, fsmonitor hooks, external diff and textconv. Queries never run Git mutations or change repository configuration. The separate, confirmed cleanup command writes a named stash and restores the selected repo’s index/working tree.

## Root-tab badge checks

The active root derives its badge from tracked status entries independently of comparison mode, search and untracked visibility. Inactive roots retain scalar results only. Checks run on opening/switching roots, focus and explicit refresh, cache for 60 seconds and stop at the first tracked change. There is no polling timer or inactive-root watcher. One badge probe shares the three-job background budget, with a five-second per-root deadline and two-second status deadline. Native blur cancels it. Discovery exclusions still apply unless a repository is explicitly added. Unavailable coverage stays stale/unknown rather than becoming clean.
