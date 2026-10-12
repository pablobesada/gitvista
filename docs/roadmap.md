# Next increments

## Diff scope selector and working-file editing — implemented

Add a compact **Cambios / Archivo completo** selector in the diff header so users can switch directly between relevant changed hunks and the complete selected file.

- Default to **Cambios**, showing modified blocks with context.
- **Archivo completo** reveals all unchanged regions while retaining original-left / updated-right alignment, syntax highlighting and change markers.
- Load full contents only when requested, retaining the 10 MiB per-side limit and virtualized rendering.
- Switching scope should preserve the selected file and the current change when possible.
- Apply to Total, Unstaged and Staged comparisons. Keep it behind `DiffView` and retain one mounted diff.

Requested and implemented on 2026-10-12. The new side can also be edited explicitly in Total and Unstaged comparisons, with guarded saving and cancellation. Prepared/index and old versions remain read-only.
