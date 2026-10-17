# GitVista

GitVista is a macOS-first multi-repository diff viewer with explicit working-file editing. Preserve the focus on reviewing changes: one active workspace root, one merged change tree, one mounted diff. Inactive root tabs retain paths, not Git snapshots, watchers or diffs. The visual reference is VS Code's side-by-side diff: original left, updated right.

## Boundaries

- `crates/core`: domain models, native paths, cancellation, memory budgets. No Tauri or frontend types.
- `crates/git`: bounded Git CLI adapter. Direct arguments, never a shell. Original path bytes stay native; frontend display paths must never become Git arguments.
- `src-tauri`: scheduling, channels, shared watchers and atomic application preferences.
- Svelte UI: immutable snapshots, virtualized tree, one Pierre CodeView. Keep the renderer behind `DiffView`.

## Invariants

- Git queries remain read-only. The only explicit Git write is the confirmed repository-level `stash_repository` action: preserve a named stash, optionally include untracked files, never ignored files or nested worktrees, and never fall back to reset/clean. Keep its native repo-ID/generation checks and close/root-switch guard. Explicit editing of the current working file uses `save_working_file`, native entry paths, disk-version checks and atomic replacement. The file editor never edits old versions, the index, symlinks or Git metadata. Keep drafts across watcher refreshes and require an explicit discard before navigation or exit.
- Preserve staged and unstaged states; a net-empty Total comparison does not mean there are no pending changes.
- Error/incomplete status is not “clean”. Preserve last-known entries and surface stale coverage.
- Background budget is three Git jobs; foreground budget is one. Cancel superseded requests and reject old generations/revisions.
- Git patches are calculated only on selection. Rust patch cache is 32 MiB; content is capped at 10 MiB per side; worker count and AST cache limits must remain explicit.
- Tests may mutate isolated temporary fixtures, never user checkouts. Source changes belong in this workspace.

## Validation

Run `npm run check`, `npm test`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `npm run bundle` as appropriate to the changed subsystem. Native watcher tests need filesystem events; frontend preview alone does not validate Tauri/WebKit. Benchmark the release build and include WebView/GPU/network processes when measuring RAM.

See README and docs/validation.md for run commands and measured baselines. Avoid adding a plugin framework, history or Git write actions to a maintenance fix.
