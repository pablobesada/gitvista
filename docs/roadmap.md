# Next increments

## Diff scope selector

Add a compact **Cambios / Archivo completo** selector in the diff header so users can switch directly between relevant changed hunks and the complete selected file.

- Default to **Cambios**, showing modified blocks with context.
- **Archivo completo** reveals all unchanged regions while retaining original-left / updated-right alignment, syntax highlighting and change markers.
- Load full contents only when requested, retaining the 10 MiB per-side limit and virtualized rendering.
- Switching scope should preserve the selected file and the current change when possible.
- Apply to Total, Unstaged and Staged comparisons. Keep it behind `DiffView` and retain one mounted diff.

Requested on 2026-10-12; deferred until after publishing the current MVP.
