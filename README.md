# GitVista

A focused desktop viewer for changes across a Git project and its nested repositories. Built with Rust, Tauri 2, Svelte 5 and a virtualized split diff. The MVP is read-only: no staging, commits, checkout or editing.

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

Root folders appear as tabs in the top bar. Use **+** or **⌘O** to add one, click its tab to switch, and **×** to close it. The clock button opens recent folders in a tab. Open tabs and the active root survive restart; reopening the same folder selects its existing tab. Arrow keys navigate a focused tab, **Ctrl+PageUp/PageDown** switch roots, and **⌘⇧W** closes the active tab. Long tab lists scroll horizontally; full paths are available in tooltips. Only the active root is scanned and watched, and its diff is released on switching. Closing the last tab returns to the welcome screen and cancels background work. Folder expansion and repository visibility remain remembered.

Choose **Carpetas** to preserve the workspace folder hierarchy or **Repositorios** to place each repository at the same level, with its own local file tree. The root repo appears first. Each repository row has a small eye toggle: an open eye shows its untracked files and folders, while a crossed-out eye hides them. The control also appears beside the root repo in folder view. Repository rows remain available when their untracked entries are hidden, so the same eye can restore them. These controls use the already loaded snapshot, keep tracked changes visible and remember their settings. Folder expansion is saved separately for each layout; hidden-only results are labeled as hidden rather than clean.

Select a file to see the original on the left and the updated version on the right. The comparison selector supports Total (HEAD → working tree), Unstaged (index → working tree) and Staged (HEAD → index). Total may be empty when staged and unstaged changes cancel; the file stays visible so the other comparisons can be inspected.

Use **F7 / Shift+F7** to move between changes, **⌘R** to reconcile the project, and click a hunk separator to reveal more context. Select text within either column or drag through line numbers to select a range; **⌘C** copies only that side, excluding gutter numbers. Roots, expanded folders, sidebar width and window geometry are remembered in application settings.

## Resource limits and behavior

- Three background Git tasks plus one foreground diff task; the foreground queue cancels stale selections.
- Only the selected diff is mounted. Both the tree and code viewport are virtualized.
- Git metadata is refreshed on filesystem events with a 200 ms debounce. Focus/manual refresh reconcile state; watcher errors are visible and do not turn a repository into “clean”.
- Rust diff cache: 32 MiB, with byte-based eviction. The highlighter uses one worker and two entries per AST cache; large or minified content falls back to plain code.
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
