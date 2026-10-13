# MVP validation — 2026-10-12

Reference machine: Apple Silicon, 32 GiB RAM, macOS 26.6, local SSD, Apple Git 2.54.0. Measurements use the optimized release application, not Vite development memory. Raw reports are under ignored `output/benchmarks/`.

## Correctness

- Type checking: zero errors and warnings; Clippy passes with warnings denied. All 39 Rust tests and 45 frontend tests pass.
- Frontend tests: tree discovery/merging, comparison filtering, 100,000 sibling rows, gitlink metadata, duplicate path ownership, distinct native names with identical display labels, selection/copy, native substrings, context hydration and external-change rejection.
- Rust tests: real Git fixtures for additions/deletions/renames, staged+unstaged cancellation, unborn HEAD, intent-to-add, submodules, worktrees/gitfiles, odd/non-UTF-8 paths, unreadable discovery subtrees, symlinks, branch-conditional filters, limits and child-process cancellation.
- Runtime integration: actual Tauri IPC serialization and FSEvents; selective updates retain unaffected repos, explicit excluded-folder repos are watched, writes during baseline are reconciled, and root switching cancels old work.
- Browser QA: searching across repos, split syntax rendering, context expansion and copying a selected old-side line. Clipboard value matched the source, including indentation and final newline. The actual Svelte DiffView also passed additions/deletions/modified layout fixtures: equal columns, additions in column two and deletions in column one, with the absent side empty.
- Native macOS QA: application opened the synthetic workspace and the real `oojs` workspace with actual Git data and syntax rendering.
- Checksums of all 172 real repositories' index/config files remained identical before and after queries and native use. No real-repository test mutation was performed.

## Release measurements

| Workload | Repos | Changes | Discovery | Status median | Status p95 | Iterations |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Actual `oojs` | 172 | 607 | 2,226 ms | 1,729 ms | 2,501 ms | 30 |
| Synthetic 10K files | 10 | 501 | 42 ms | 147 ms | 389 ms | 10 |
| Synthetic 100K files | 100 | 5,000 | 402 ms | 1,100 ms | 2,538 ms | 10 |

The extra synthetic change is the dedicated watcher fixture file. Actual counts include individual untracked files, unlike a status scan that groups untracked directories. Status samples share a single adapter instance; the first sample initializes the configuration cache. Discovery is measured separately and is required when opening a new root, not on every incremental refresh. Combining discovery with median status is a warm approximation, not a measured cold-start percentile.

The first implementation launched Git sequentially to inspect repository locations and took 11,816 ms to discover `oojs`. Filesystem-based gitdir/common-dir/HEAD inspection reduced that measurement to 2,226 ms. Baselines were collected on a working machine, so filesystem cache and other running software may affect results.

The sample actual diff took 55 ms at the backend (395-byte patch); synthetic sample diffs took 99–106 ms. These are individual samples, not a proven p95 for all files up to 1 MiB, and exclude frontend rendering.

A repeat using the final backend returned 1,920 ms status median and 4,698 ms p95 across 10 `oojs` runs, with 2,068 ms discovery and a 58 ms sample diff. It began concurrently with the native app's initial scan, so the first iterations shared CPU/filesystem resources. Raw report: `oojs-shipped.json`.

During a 30-second `oojs` idle session with the final backend, the main application plus its newly launched WebContent/GPU/Networking processes averaged **121.0 MiB RSS**, peaked at **130.7 MiB**, and reported **0.0% CPU**. No Git jobs were active in that idle sample; transient scan peaks are not included. This sample preceded the final CSS correction for one-sided diffs. An earlier 31-second session averaged 126.5 MiB RSS and 0.02% reported CPU.

The `.app` bundle is approximately **8.9 MiB**. Languages/themes are loaded on demand. Large grammar chunks on disk do not mean they are all loaded into memory.

## Repository presentation update

Both folder and peer-repository layouts are covered by native-key, ownership, comparison, search and per-repo untracked filtering tests. Legacy preference files migrate without losing recent roots or expansion state, and the new settings round-trip through native atomic storage. Switching layouts and visibility does not change Git queries or worker/cache budgets. Inline eye toggles remain available for hidden-only repositories in both layouts; only visible tree rows are mounted.

Browser QA verified `oojs` and `opencustom` as peer groups, independent filtering while retaining tracked changes, the unchanged selected diff, and persistence after reload. Native macOS QA of the initial controls verified both layouts over 172 repositories; hiding all 607 untracked entries displayed an explicit hidden-files state, then re-enabling restored them. Native preferences contained the new layout/visibility fields while preserving existing roots and expansion settings.

The final interaction uses inline eye buttons instead of a picker. Browser QA confirmed that the eye changes visibility without collapsing the repo or changing the selected diff. Hidden-only repository headers/skeletons are covered by regression tests, including restoration and search behavior. File activation and visibility are separate accessible buttons, and tree item names remain available to keyboard navigation. The final native bundle displayed the root eye while preserving the active workspace and sidebar preferences.

## Root tabs

Four frontend regression tests cover legacy/CLI restoration, deduplication of canonical aliases, closing inactive tabs and choosing the adjacent active tab. Native integration closes a root during a pending refresh, rejects its cached entry, verifies no subsequent events, then reopens it and observes edits made while closed. Preferences round-trip the opened roots without losing existing layout, visibility or expansion settings.

Browser QA verified adding two roots, switching tabs, closing inactive and last tabs, returning to the welcome screen, keyboard arrow navigation, and restoring both tabs plus the active root after reload. The optimized native bundle displayed `oojs` and `oojs3` as separate tabs, switched between them using actual Git data, and persisted both paths. Existing sidebar width and per-repository settings were retained. Only the selected root has watchers or Git snapshots; previous performance baselines were not remeasured for this UI change.

## Full-file review and editing

Frontend tests cover complete versions for net-empty comparisons, pure renames, additions/deletions, empty documents, no-final-newline metadata and unavailable sides. Existing context validation and copying tests remain green. Browser QA exercised complete old/new rendering, editing the new side, protected comparison navigation, discarding a draft and saving to the preview model. The original source text stayed unchanged.

Native tests save only isolated fixture files. They verify disk-version rejection, workspace-generation rejection, UTF-8/size and native-path guards, symlink rejection, atomic replacement, CRLF and executable-mode preservation, temporary-file cleanup, cache invalidation and selective watcher updates. Git index/config bytes are checked around explicit saves. The release app's ⌘S also wrote an actual test file while its HEAD version stayed unchanged. No user checkout was edited during QA.

Final native macOS QA displayed both complete versions for a net-empty Total comparison and edited an initially empty, untracked file. ⌘Q displayed the dirty-draft dialog; choosing to continue retained the exact draft, and ⌘S saved it without changing index/config checksums or leaving temporary files. The test root was closed afterward and the application returned to `oojs`.

The new scope and editor retain virtualization and existing budgets. Full-view/editor RAM, startup and scrolling percentiles have not been rebenchmarked; the earlier release measurements describe the review MVP.

## Block restoration — 2026-10-13

Fifteen frontend tests cover separate consecutive changes within one Git hunk, partial patch coordinates, insertions/deletions at file boundaries, empty versions, CRLF/Unicode, final-newline changes, stale block rejection and whole-file undo/redo. Browser QA verifies the inline arrow, live block restoration while retaining another manual edit, whole-file restoration, separate undo history, redo, discard protection and disabled actions for Prepared/index. Restore actions create a draft and require an explicit save through the existing native writer.

Native macOS QA restores two blocks within the same Git hunk from the index version, undoes only the last operation, redoes and saves the exact two-block result. A deleted-line block can be restored, and scrolling reveals the correctly positioned action for an initially offscreen block. Whole-file restoration matches the index version and keeps staged changes visible in Prepared. Test files remain untouched before explicit saving. All writes use isolated fixtures; no user checkout is edited.

An additional native regression test restores a file until its contents match the index, then queries Staged and Unstaged while checking index/config bytes. It exposed Git porcelain diff's automatic stat refresh despite optional locks being disabled. The adapter now passes `-c diff.autoRefreshIndex=false` on its commands, without writing repository configuration; the regression test verifies unchanged index/config bytes after saving and follow-up queries.

The final release bundle also passed the same whole-file restoration/save case in WebKit: working contents matched the index version, index/config checksums remained identical, and no temporary save files remained. The QA root was closed and `oojs` was restored as active while preserving the user's `oojs2` tab.

## Full-file change overview — 2026-10-13

Six frontend tests cover proportional placement across distant hunks and unchanged tails, split padding for deletions, additions at EOF, empty/identical versions and minimum thumb size at both scroll boundaries. `npm run check`, all 51 frontend tests and the macOS release bundle pass.

Browser QA verifies red/green marks, click-to-block navigation, keyboard navigation and live updates in a long preview-only draft. The overview uses one bounded canvas alongside the existing single virtualized diff. Native WebKit QA verifies the full-file overview on a real added file, the thumb reaching 100% at EOF and keyboard navigation returning to the change block. Native QA uses only read operations; draft changes were confined to browser preview data. UI RAM and frame rates were not remeasured for this addition.

## Root-tab change badges — 2026-10-13

The active badge uses raw workspace entries independently of comparison mode, search and hidden untracked entries. Inactive roots retain only scalar badge results, checked on opening/switching roots, focus or explicit refresh. Cached checks expire after 60 seconds but do not schedule a timer. A single cancellable badge probe shares the three-job background budget; root work is limited to five seconds and status to two seconds. Native blur cancels the probe. No inactive-root watcher, file snapshot or patch is retained.

Validation passes: 54 frontend tests, 43 Rust tests, Clippy with warnings denied, Svelte checks and the release app bundle. Fixture tests verify ignored nested repositories, explicitly added generated-directory repositories, subdirectory scope, cached/forced refresh, unknown errors, filter suppression, staged/unstaged cancellation, unchanged Git index/config and releasing both job permits on cancellation. Querying dormant roots leaves the active workspace generation and entries unchanged. Browser QA checks both active/inactive red dots and preservation while filtering all entries away. Native WebKit QA displays change indicators for `oojs`, `oojs2` and `oojs-skills` without switching to the inactive roots.

A 15-second idle sample with those three tabs reports 0.0% mean CPU across the app plus its identified WebContent, GPU and Networking processes, with four processes throughout. Total RSS averages 208.2 MiB (peak 208.5 MiB) with a selected highlighted diff; this measures the whole app, not the incremental memory of badges. An earlier 10-second sample measured 137.9 MiB with different displayed content and is not a controlled memory comparison. The release adapter benchmark on GitVista (five iterations, three repositories) reports status median 26.1 ms and p95 57.9 ms; it measures the adapter, not badge latency or UI frames.

## Remaining performance characterization

The product functionality is implemented and tested. The goals of startup ≤1 second, scroll at 60 fps, and end-to-end diff p95 ≤250 ms still need a dedicated native frame/startup benchmark; they are not claimed as measured guarantees. Keep these acceptance goals in future profiling rather than inferring them from Rust, Tauri or virtualized DOM alone.

## Reproduce

```sh
npm ci
npm run bundle
target/release/gitvista --benchmark /Users/pablo/desarrollos/oojs 30
python3 scripts/create-fixture.py /tmp/new-gitvista-100k --files 100000 --repos 100 --changed 5000
target/release/gitvista --benchmark /tmp/new-gitvista-100k 10
```

The fixture generator rejects existing destinations. Public signing/notarization, Intel builds, Windows and Linux are outside this macOS Apple Silicon MVP release.

For idle process accounting, identify the app PID and its newly created WebContent/GPU/Networking PIDs at launch, then run `python3 scripts/profile-idle.py MAIN_PID GPU_PID NETWORK_PID CONTENT_PID --seconds 30 --workspace oojs`. The script includes descendant processes such as transient Git jobs; CPU is the mean of `ps`-reported percentages sampled every second. Run on a quiet machine to avoid attributing another app's new WebKit processes to GitVista.
