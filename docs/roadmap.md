# Roadmap

GitVista is an early MVP focused on fast review across nested repositories. This document distinguishes shipped capabilities from candidate work; it is not a release schedule or a promise of dates.

## Available now

- Downloadable macOS Apple Silicon ZIP releases with checksums, ad hoc signatures and installation instructions.
- Automatic nested-repo, worktree and submodule discovery, plus explicit additions under excluded directories.
- One merged change tree, folder/peer-repository layouts, search and per-repo untracked visibility.
- Root tabs with one active workspace and lightweight tracked-change indicators.
- Split Total/Unstaged/Staged comparisons, relevant blocks or complete files, syntax highlighting, inline long-line emphasis and optional word wrap.
- Explicit current-file editing, guarded saving, block/whole-file restoration into drafts and undo/redo.
- Confirmed per-repository stash cleanup with optional untracked files and a recovery command.
- Incremental refresh, bounded Git scheduling, virtualized rendering and reproducible fixture tests.

See [usage](usage.md) for behavior and [validation](validation.md) for measured results and limits. Native validation currently covers macOS Apple Silicon.

## Near-term priorities

1. Keep correctness and responsiveness stable as workspaces grow: preserve drafts, stale-state indicators, native path identities and clear write boundaries.
2. Complete native startup, frame/scroll and end-to-end diff measurements. The existing latency/frame goals remain goals until measured.
3. Improve onboarding and diagnose reported setup problems using reproducible, sanitized examples.

## Candidate increments

These need implementation and validation before they can be offered:

- Developer ID signing and Apple notarization for macOS releases.
- CI checks for contributors; no automated GitHub workflow is currently configured.
- Linux, Intel Mac and Windows compatibility work, including native watcher, renderer, clipboard and build verification.
- A compact recovery UI for GitVista-created stashes. Recovery currently uses the displayed Git command; there is no stash-history UI.
- Accessibility, keyboard and language improvements based on concrete user reports. Current UI labels are in Spanish.

## Scope to discuss before implementation

Staging, commits, history, conflict resolution, multiple simultaneously active workspace snapshots and a plugin framework would expand the product substantially. They are not current features or committed milestones. Propose a focused use case through [GitHub Issues](https://github.com/pablobesada/gitvista/issues/new/choose) before starting this work.
