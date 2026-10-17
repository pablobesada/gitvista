# Usage guide

GitVista reviews changes across a root folder and its nested Git repositories. The desktop application uses your installed Git. The browser preview uses bundled sample data and cannot inspect local repositories or perform repository cleanup.

The validated desktop target is macOS Apple Silicon. UI labels are currently in Spanish; this guide gives their English meanings. For installation, see the [README](../README.md#build-and-run-on-macos).

## Open roots and discover repositories

Open a project folder with **⌘O** or the **+** button in the top bar. GitVista discovers repositories in the background and shows results progressively. A root can contain a main repository, nested independent repositories, worktrees and submodules.

Generated directories such as `node_modules`, `target` and `.venv` are excluded from automatic discovery. Use **Agregar repositorio** (Add repository) to explicitly add a repository inside an excluded directory. It must be under the active workspace root. Discovery exclusions do not define Git's tracked/untracked status or hide changes in an already discovered repo.

Root folders appear as tabs. Click a tab to switch; **×** closes it, and the clock button opens recent folders. Open tabs, the active root, sidebar width, folder expansion, untracked visibility and window geometry are remembered. Only one root has a full live workspace at a time. Inactive tabs retain paths and lightweight change indicators.

## Navigate the change tree

Choose the organization above the tree:

| Control | Meaning |
| --- | --- |
| **Carpetas** | Preserve the workspace folder hierarchy, with nested repositories marked as repositories. |
| **Repositorios** | Show repositories as peer groups, each with its own local file tree. |
| Search / **⌘P** | Filter changed files by name, path or repository. |
| Eye icon on a repo | Show or hide that repository's untracked files and folders. Tracked changes remain visible. |

The two layouts share the same changes, with folder expansion remembered separately. Hidden-only repositories keep a reachable eye control; a hidden-files message does not mean the repo is clean. Search and comparison mode also affect the visible tree.

Select a file to display one split diff. Use the tree's arrow keys to navigate and expand/collapse folders, or **Alt+↑ / Alt+↓** to move between visible changed files.

### Root change indicators

- A **red dot** means tracked changes were found, including staged changes and nested repositories. Untracked-only files do not activate the dot, whether visible or hidden.
- An **outlined amber dot** means coverage is pending, stale or unavailable. Check the tooltip/error; unknown status is not a clean result.
- An inactive tab describes its last check. It is refreshed on relevant root/focus actions or **⌘R**, not continuously watched.

Filesystem events refresh affected repositories. Focus and manual refresh reconcile state. Refresh preserves the selected file and unsaved drafts where possible. If Git cannot be queried, GitVista retains the last-known entries and surfaces the error.

## Choose a comparison

The left side is the original; the right side is the updated version for the selected comparison:

| UI label | Left | Right | Editable right side? |
| --- | --- | --- | --- |
| **Total** | HEAD | Current working file | Yes, through explicit editing |
| **Sin preparar** (Unstaged) | Index | Current working file | Yes, through explicit editing |
| **Preparado** (Staged) | HEAD | Index | No |

A new repository without HEAD uses an empty original for review. A Total diff can be empty when prepared and unprepared changes cancel each other; the file remains in the tree so both comparisons can still be inspected.

### Changed blocks or complete files

- **Cambios** shows modified blocks with context. Click a hunk separator to reveal more context.
- **Archivo completo** loads both complete versions for the selected file, within the content limits below.
- **Ajustar líneas** toggles word wrap in both columns. It starts off, retains its setting while switching files during the session, and does not change file contents.
- Stronger red/green emphasis marks changed fragments inside a line. For dense or widely separated edits in very long lines, emphasis may cover the differing middle rather than each individual edit; identical ends remain unmarked.

Full-file view has a change overview on the right. Red marks indicate deletions, green marks additions, and the shaded frame shows the viewport. Click a mark to jump to its block. Focus the overview and use arrow keys to move between changes, PageUp/PageDown to scroll, or Home/End to reach the ends.

Use **F7 / Shift+F7** for next/previous change while reviewing. Select text in either column, or drag through its line numbers to select a range; **⌘C** copies that side without gutter numbers. Line-number ranges can span virtualized rows. During editing, ordinary copy and navigation belong to the editor.

## Edit the current working file

In Total or Sin preparar, select **Editar versión nueva** (Edit updated version). The right side becomes an editable draft; the left version stays read-only. Use **Guardar** or **⌘S** to save explicitly.

Unsaved drafts are retained through refreshes. Navigating away, changing roots/comparisons, canceling the editor or quitting asks you to discard the draft or continue editing. Saving checks the disk version that was loaded. If another program changed the file, the save is rejected and the draft remains available rather than silently overwriting the external change.

Supported writes are UTF-8 regular files up to 10 MiB. The file editor does not edit Git's index, old versions, symlinks, binary files or filtered/unsupported-encoding files. Saving atomically replaces the working file and preserves permission mode; it does not stage changes. Disk-version checks are optimistic concurrency control, not a lock against all concurrent external writers.

### Restore a block or whole-file content

The **→** button between the columns restores that consecutive change block from the left version into the current-file draft, preserving neighboring changes. **Restaurar archivo** (Restore file) replaces the entire draft with the left version.

These actions create draft edits; they do not write immediately. Review the result, use **Deshacer / Rehacer** or **⌘Z / Shift+⌘Z**, then save or cancel. Each restoration has its own undo step in the editor's bounded 50-entry history.

The source is HEAD in Total and the index in Sin preparar. Restoration is unavailable in Preparado. Whole-file restoration needs both versions to exist; missing current files are not recreated. Restoring an added block removes its text from the draft rather than deleting the file. These are content actions, not branch resets or index restoration.

## Save changes in a stash and clean a repository

Right-click a **repository header** in either tree layout and choose **Guardar en stash y limpiar…** (Save in stash and clean). This is an explicit Git write action affecting the **whole selected repository**, even if the workspace root is a subfolder of it. Ordinary folder rows are not repository cleanup targets.

The confirmation shows the full repository path and counts of tracked, prepared, unprepared and untracked files, independently of the current search, comparison or eye toggle. Prepared and unprepared counts can overlap for the same file. **Incluir archivos sin seguimiento** includes untracked files by default; uncheck it to leave them on disk.

On confirmation, GitVista runs `git stash push`, optionally with `--include-untracked`. It saves changes, restores the selected repo's working tree and index toward HEAD, and refreshes the repo. Ignored files and the contents of independent nested working trees are preserved. Cleaning a parent does not clean its child repositories.

GitVista checks the repository generation and confirmation fingerprint before writing. It rejects stale confirmations, unresolved conflicts, in-progress Git operations, repositories without a first commit and configured Git/LFS filters. A parent that tracks changed files inside another repository is rejected rather than cleaning that child indirectly. Wait for an active status query to finish before confirming cleanup.

Navigation/exit are guarded while Git is writing. If Git creates a backup but cleanup fails or leaves changes, the result shows a warning and preserves the stash identifier. There is no automatic `reset --hard`, forced recursive clean or stash deletion fallback. Read any warning before retrying.

### Recover the saved changes

The result provides the stash ID and a command to copy. **In a terminal located in that repository**, run the displayed command, using its actual object ID:

```sh
git stash apply --index STASH_OBJECT_ID
```

`--index` attempts to restore the prepared state as well as the working changes. Applying can conflict with changes made since cleanup; inspect and resolve those in Git. `apply` keeps the stash, unlike removing it after recovery. GitVista currently provides no stash-history or recovery UI. See the upstream [Git stash documentation](https://git-scm.com/docs/git-stash) for recovery details.

## Keyboard reference

These are the macOS bindings exposed by the UI. An editor's active text selection/undo behavior takes precedence while editing.

| Shortcut | Action |
| --- | --- |
| **⌘O** | Add/open a root folder |
| **⌘P** | Focus file/repository search |
| **⌘R** | Refresh workspace and root indicators |
| **Ctrl+PageUp / Ctrl+PageDown** | Switch root tabs |
| **⌘Shift+W** | Close the active root tab |
| **Arrow keys / Enter** | Navigate/activate the focused change tree |
| **Alt+↑ / Alt+↓** | Select the previous/next visible changed file |
| **Shift+F10 / Context Menu key** | Open actions for a focused repository row in the change tree |
| **F7 / Shift+F7** | Next/previous change while reviewing |
| **⌘C** | Copy the selected text or line range on one side |
| **⌘S** | Save the active working-file draft |
| **⌘Z / Shift+⌘Z** | Undo/redo the active editor draft |

## Limits and unavailable states

Complete contents are capped at **10 MiB per side**. Binary files, conflicts, Git metadata, external filters and unsupported encodings can produce informational states rather than an editable two-way text diff. Git command output, runtime, cached patches and renderer work are bounded. A failed or incomplete query is never evidence that there are no changes.

There is no staging, commit, history or conflict-resolution UI. For technical limits and measurements, see [architecture](architecture.md) and [validation](validation.md). Report ordinary problems through [GitHub Issues](https://github.com/pablobesada/gitvista/issues/new/choose), using sanitized examples; report security problems through the [private channel](../SECURITY.md).
