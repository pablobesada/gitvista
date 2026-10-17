# GitVista

A focused desktop viewer for changes across a Git project and its nested repositories. Built with Rust, Tauri 2, Svelte 5 and a virtualized split diff. Git operations are read-only: no staging, commits or checkout. Editing the selected current working file is an explicit, separate action.

## Run

Requirements: Node.js 22.12+ and Rust 1.99.0. On macOS, install Xcode command-line tools. The project pins Rust and all npm dependencies; Cargo.lock pins the Rust dependency graph.

```sh
npm ci
npm run desktop
```

The npm Tauri wrapper also finds Rust in `~/.cargo/bin` without requiring edits to your shell profile. `npm run dev` runs a clearly labeled browser preview with sample data; filesystem access is available only in the desktop application.

```sh
npm run bundle
open target/release/bundle/macos/GitVista.app
```

The local `.app` build does not require a signing account. Public distribution and notarization are a separate release step.

## Review a project

Open a root folder with **⌘O**. GitVista discovers its Git repositories, including nested repos, worktrees and submodules. Search paths or repo names with **⌘P**. A single tree displays changed files and their parent folders. Generated directories are excluded from discovery; **Add repository** lets you explicitly include a repo under one of them.

Root folders appear as tabs in the top bar. Use **+** or **⌘O** to add one, click its tab to switch, and **×** to close it. The clock button opens recent folders in a tab. Open tabs and the active root survive restart; reopening the same folder selects its existing tab. Arrow keys navigate a focused tab, **Ctrl+PageUp/PageDown** switch roots, and **⌘⇧W** closes the active tab. Long tab lists scroll horizontally; full paths are available in tooltips. Only the active root retains a Git snapshot and watchers, and its diff is released on switching. Closing the last tab returns to the welcome screen and cancels background work. Folder expansion and repository visibility remain remembered.

A small **red dot** marks roots with changes, including automatically discovered nested repositories and explicitly added repositories. It includes staged, unstaged and untracked changes regardless of search, comparison mode or hidden entries. An **outlined amber dot** means the state is pending or unavailable; the tooltip describes coverage and last-known state. The active tab reuses normal workspace events. Inactive tabs retain only a scalar badge: probes run when opening/switching roots or returning to the window, reuse results for 60 seconds, and stop on the first changed repo. **⌘R** forces a refresh of all open roots. There is no recurring timer or inactive-root watcher. Only one badge probe runs at a time within the existing three-job background budget; each root has a five-second deadline and Git status has a two-second deadline. Blurring the native window cancels badge work. Discovery keeps its usual generated-directory exclusions; a repository inside those directories participates when explicitly added. Badge status output is drained without retaining file paths or building entries/patches. An inactive badge describes the last check rather than continuous monitoring.

Choose **Carpetas** to preserve the workspace folder hierarchy or **Repositorios** to place each repository at the same level, with its own local file tree. The root repo appears first. Each repository row has a small eye toggle: an open eye shows its untracked files and folders, while a crossed-out eye hides them. The control also appears beside the root repo in folder view. Repository rows remain available when their untracked entries are hidden, so the same eye can restore them. These controls use the already loaded snapshot, keep tracked changes visible and remember their settings. Folder expansion is saved separately for each layout; hidden-only results are labeled as hidden rather than clean.

Select a file to see the original on the left and the updated version on the right. The comparison selector supports Total (HEAD → working tree), Unstaged (index → working tree) and Staged (HEAD → index). Total may be empty when staged and unstaged changes cancel; the file stays visible so the other comparisons can be inspected.

Changed fragments have stronger red/green emphasis inside the line, including long minified lines and source maps. **Ajustar líneas** toggles word wrap in both columns, including while editing. It starts off and retains its current setting while switching files during the session; wrapping changes only the display, not file contents.

Choose **Cambios / Archivo completo** above the diff to show changed blocks or both complete versions. Full contents are loaded lazily and retain split alignment and virtualization. Click **Editar versión nueva** to edit the right column, then **Guardar** or **⌘S** to save to disk. The original left column stays read-only. **Cancelar** discards the edit session; unsaved drafts require confirmation before navigating, switching roots/comparisons or quitting. External refreshes preserve the draft, and saving checks the original disk version so concurrent edits are not silently replaced.

**Archivo completo** includes a narrow change overview on the right: red marks show deleted lines, green marks show added lines, and a shaded frame shows the visible area. Click a mark to center its change block or click elsewhere to jump through the file. Focus the overview and use **↑ / ↓** to move between changes, **PageUp / PageDown** to scroll, or **Home / End** to reach the ends. Marks update with the current draft while editing. The overview uses one small canvas and the existing diff metadata, retaining a single virtualized code viewport.

Editing applies to **Total** and **Sin preparar**, where the right column is the current working file. **Preparado** shows the index and remains read-only. UTF-8 regular files up to 10 MiB are supported; missing, binary, filtered and symlink files cannot be edited. Saving uses the file's native path, replaces it atomically and preserves its permission mode. It never stages changes or modifies Git's index/configuration. Undo and redo use the editor's standard shortcuts; the active editor has a bounded 50-entry undo history and is released when leaving the file.

Each visible change block has a small **→** button between the columns. It restores only that consecutive block from the left side into the current-file draft, keeping neighboring changes and edits. **Restaurar archivo** replaces the entire draft with the left version. The source is HEAD in Total and the index in Sin preparar. Neither action writes immediately: review the draft, use **Deshacer / Rehacer** or **⌘Z / ⇧⌘Z**, then save with **⌘S** or cancel. Each restoration has its own undo step. Whole-file restoration requires both versions to exist; restoring an added block removes its text from the draft without deleting the file. Missing current files, metadata and Git's index are not restored by these content actions.

Use **F7 / Shift+F7** to move between changes, **⌘R** to reconcile the project, and click a hunk separator to reveal more context. Select text within either column or drag through line numbers to select a range; **⌘C** copies only that side, excluding gutter numbers. Roots, expanded folders, sidebar width and window geometry are remembered in application settings.

## Resource limits and behavior

- Three background Git tasks plus one foreground diff task; the foreground queue cancels stale selections.
- Only the selected diff is mounted. Both the tree and code viewport are virtualized.
- Git metadata is refreshed on filesystem events with a 200 ms debounce. Focus/manual refresh reconcile state; watcher errors are visible and do not turn a repository into “clean”.
- Rust diff cache: 32 MiB, with byte-based eviction. The highlighter uses one worker and two entries per AST cache; large or minified content falls back to plain code.
- Inline changes remain visible beyond the 2,000-character syntax limit. Long lines trim identical ends before character comparison, with a 32,768-character combined middle, 256-edit and 20 ms refinement budget. If refinement exceeds a budget, emphasis covers the differing middle while leaving the identical ends unmarked. The pinned Pierre adapter applies in the worker and local/editor renderer and fails explicitly if its integration point changes.
- Full context is loaded only for the selected file, up to 10 MiB per side. Patches, command time and stdout are bounded.
- Binary files, merge conflicts, external content filters and unsupported encodings have explicit informational states.
- Git commands use direct arguments, NUL-delimited status and original native paths. They disable optional index locks, fsmonitor hooks, external diff and textconv. The app never runs a Git mutation or changes repository configuration.

## Verify

```sh
npm run check
npm test
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm run build
```

After building, benchmark the actual shipped Git adapter without opening a window:

```sh
target/release/gitvista --benchmark /path/to/project 30
```

This reports discovery time, status median/p95 with three workers, changes and one sample diff. It performs only read queries. The scripts directory includes a synthetic fixture generator for 10K/100K-file workloads. End-to-end performance must additionally include the WebView and transient Git processes.

## Extend

`crates/core` owns domain types, cancellation and cache limits. `crates/git` implements `GitBackend` with the installed Git. `src-tauri` owns scheduling, watchers, channels and settings. `src/lib/bridge.ts` is the typed boundary consumed by the Svelte UI. `DiffView` encapsulates the third-party renderer.

Keep future staging/history operations in the Git adapter, UI features in components, and resource budgets in the core. Avoid leaking Tauri or renderer types into the domain. See [architecture](docs/architecture.md) for the invariants and [validation](docs/validation.md) for measured results and known limits.

## License

MIT. Third-party dependencies retain their own licenses, including Apache-2.0 for `@pierre/diffs`.
