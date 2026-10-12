import { parsePatchFiles, setCustomExtension } from '@pierre/diffs';
import type { FileDiffMetadata, SelectedLineRange, SelectionSide } from '@pierre/diffs';
import type { DiffContents, DiffResponse } from './types';

setCustomExtension('es6', 'javascript');

export interface ChangeTarget { lineNumber: number; side: SelectionSide; }
export interface TextPoint { line: number; character: number; side: SelectionSide; }
export interface TextRange { start: TextPoint; end: TextPoint; }

/** Ensure lazy context still describes the displayed patch, including omitted gaps. */
export function validateLoadedContents(diff: FileDiffMetadata, contents: DiffContents): void {
  if (!diff.isPartial || diff.type === 'rename-pure') return;
  const old = new LineCursor(contents.oldFile?.contents ?? '');
  const next = new LineCursor(contents.newFile?.contents ?? '');
  const changed = () => { throw new Error('El archivo cambió mientras se cargaba el contexto. Actualizá el diff para continuar.'); };
  for (const hunk of diff.hunks) {
    // A zero-length side points after its numbered line; a nonempty side starts on it.
    const oldStart = hunk.deletionCount === 0 ? hunk.deletionStart : Math.max(0, hunk.deletionStart - 1);
    const newStart = hunk.additionCount === 0 ? hunk.additionStart : Math.max(0, hunk.additionStart - 1);
    const oldGap = oldStart - old.line;
    const newGap = newStart - next.line;
    if (oldGap < 0 || oldGap !== newGap) changed();
    for (let index = 0; index < oldGap; index++) {
      const oldLine = old.read();
      const newLine = next.read();
      if (oldLine == null || oldLine !== newLine) changed();
    }
    for (let index = 0; index < hunk.deletionCount; index++) {
      if (old.read() !== diff.deletionLines[hunk.deletionLineIndex + index]) changed();
    }
    for (let index = 0; index < hunk.additionCount; index++) {
      if (next.read() !== diff.additionLines[hunk.additionLineIndex + index]) changed();
    }
  }
  while (true) {
    const oldLine = old.read();
    const newLine = next.read();
    if (oldLine !== newLine) changed();
    if (oldLine == null) break;
  }
}

class LineCursor {
  line = 0;
  private offset = 0;
  constructor(private readonly text: string) {}
  read(): string | undefined {
    if (this.offset >= this.text.length) return undefined;
    const start = this.offset;
    const newline = this.text.indexOf('\n', start);
    this.offset = newline < 0 ? this.text.length : newline + 1;
    this.line++;
    return this.text.slice(start, this.offset);
  }
}

/** Parse once per revision. The renderer hydrates this same object in place. */
export function parseSelectedDiff(response: DiffResponse): FileDiffMetadata | null {
  if (response.result.kind !== 'text') return null;
  const key = `${response.generation}:${response.requestId}:${response.result.entryId}:${response.result.mode}`;
  const files = parsePatchFiles(response.result.patch, key, true).flatMap((patch) => patch.files);
  if (files.length !== 1) throw new Error(`Se esperaba un archivo en el diff; Git devolvió ${files.length}.`);
  return files[0];
}

/** Navigate to the change itself, rather than the context preceding the hunk. */
export function changeTargets(diff: FileDiffMetadata): ChangeTarget[] {
  return diff.hunks.flatMap((hunk) => {
    let oldLine = Math.max(1, hunk.deletionStart);
    let newLine = Math.max(1, hunk.additionStart);
    for (const content of hunk.hunkContent) {
      if (content.type === 'context') {
        oldLine += content.lines;
        newLine += content.lines;
      } else {
        return [{
          lineNumber: content.additions > 0 ? newLine : oldLine,
          side: content.additions > 0 ? 'additions' as const : 'deletions' as const,
        }];
      }
    }
    return [];
  });
}

/** Null means that the range crosses context absent from a partial Git patch. */
export function selectedLinesText(diff: FileDiffMetadata, range: SelectedLineRange): string | null {
  const side = range.side ?? 'additions';
  if (range.endSide != null && range.endSide !== side) return null;
  const first = Math.min(range.start, range.end);
  const last = Math.max(range.start, range.end);
  if (first < 1 || !Number.isInteger(first) || !Number.isInteger(last)) return null;
  const lines = side === 'deletions' ? diff.deletionLines : diff.additionLines;
  if (!diff.isPartial) {
    return last <= lines.length ? lines.slice(first - 1, last).join('') : null;
  }
  const selected: string[] = [];
  let next = first;
  for (const hunk of diff.hunks) {
    const start = side === 'deletions' ? hunk.deletionStart : hunk.additionStart;
    const count = side === 'deletions' ? hunk.deletionCount : hunk.additionCount;
    const index = side === 'deletions' ? hunk.deletionLineIndex : hunk.additionLineIndex;
    const end = start + count - 1;
    if (end < next) continue;
    if (start > next) return null;
    const stop = Math.min(end, last);
    selected.push(lines.slice(index + next - start, index + stop - start + 1).join(''));
    next = stop + 1;
    if (next > last) return selected.join('');
  }
  return null;
}

/** Slice raw contents without constructing an array for the entire file. */
export function contentsLinesText(contents: string, start: number, end: number): string | null {
  const first = Math.min(start, end);
  const last = Math.max(start, end);
  if (first < 1 || !Number.isInteger(first) || !Number.isInteger(last)) return null;
  let line = 1;
  let cursor = 0;
  while (line < first) {
    const newline = contents.indexOf('\n', cursor);
    if (newline < 0) return null;
    cursor = newline + 1;
    line++;
  }
  const from = cursor;
  while (line <= last) {
    if (cursor >= contents.length) return null;
    const newline = contents.indexOf('\n', cursor);
    if (newline < 0) return line === last ? contents.slice(from) : null;
    cursor = newline + 1;
    line++;
  }
  return contents.slice(from, cursor);
}

export function selectionFromContents(contents: DiffContents, range: SelectedLineRange): string | null {
  const side = range.side ?? 'additions';
  if (range.endSide != null && range.endSide !== side) return null;
  const file = side === 'deletions' ? contents.oldFile : contents.newFile;
  return file == null ? null : contentsLinesText(file.contents, range.start, range.end);
}

export function nativeSelectionText(diff: FileDiffMetadata, range: TextRange): string | null {
  if (range.start.side !== range.end.side) return null;
  let { start, end } = range;
  if (start.line > end.line || (start.line === end.line && start.character > end.character)) {
    [start, end] = [end, start];
  }
  const raw = selectedLinesText(diff, { start: start.line, end: end.line, side: start.side });
  if (raw == null) return null;
  const endLineStart = start.line === end.line ? 0 : raw.lastIndexOf('\n', raw.endsWith('\n') ? raw.length - 2 : raw.length - 1) + 1;
  return raw.slice(start.character, endLineStart + end.character);
}

/** Resolve native text endpoints in open shadow DOM. Gutters never enter the model. */
export function nativeTextRange(root: HTMLElement): TextRange | null {
  const shadows = Array.from(root.querySelectorAll('diffs-container')).flatMap((element) => element.shadowRoot == null ? [] : [element.shadowRoot]);
  const selections: Selection[] = [];
  for (const shadow of shadows) {
    const selection = (shadow as ShadowRoot & { getSelection?: () => Selection | null }).getSelection?.();
    if (selection != null) selections.push(selection);
  }
  const documentSelection = root.ownerDocument.getSelection();
  if (documentSelection != null) selections.push(documentSelection);
  for (const selection of selections) {
    if (selection.isCollapsed || selection.rangeCount === 0) continue;
    let startNode = selection.anchorNode;
    let startOffset = selection.anchorOffset;
    let endNode = selection.focusNode;
    let endOffset = selection.focusOffset;
    let composed: StaticRange[] | undefined;
    try {
      composed = (selection as Selection & {
        getComposedRanges?: (options: { shadowRoots: ShadowRoot[] }) => StaticRange[];
      }).getComposedRanges?.({ shadowRoots: shadows });
    } catch { /* Older WebViews expose the preceding signature; native endpoints remain usable. */ }
    if (composed?.[0] != null) {
      startNode = composed[0].startContainer;
      startOffset = composed[0].startOffset;
      endNode = composed[0].endContainer;
      endOffset = composed[0].endOffset;
    }
    const start = pointInCode(startNode, startOffset, shadows);
    const end = pointInCode(endNode, endOffset, shadows);
    if (start != null && end != null && start.side === end.side) return { start, end };
  }
  return null;
}

function pointInCode(node: Node | null, offset: number, shadows: ShadowRoot[]): TextPoint | null {
  if (node == null || !shadows.includes(node.getRootNode() as ShadowRoot)) return null;
  const element = node instanceof Element ? node : node.parentElement;
  const line = element?.closest<HTMLElement>('[data-line]');
  const code = line?.closest<HTMLElement>('[data-code]');
  if (line == null || code == null) return null;
  const side = code.hasAttribute('data-deletions') ? 'deletions' : code.hasAttribute('data-additions') ? 'additions' : null;
  const lineNumber = Number(line.dataset.line);
  if (side == null || !Number.isInteger(lineNumber)) return null;
  const prefix = node.ownerDocument?.createRange();
  if (prefix == null) return null;
  prefix.setStart(line, 0);
  prefix.setEnd(node, offset);
  return { line: lineNumber, character: prefix.toString().length, side };
}
