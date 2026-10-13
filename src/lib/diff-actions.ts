import type { FileDiffMetadata, SelectionSide } from '@pierre/diffs';
import type { Position, TextEdit } from '@pierre/diffs/edit';

/** One consecutive change, excluding nearby unchanged context in the Git hunk. */
export interface ChangeBlock {
  key: string;
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
  oldIndex: number;
  newIndex: number;
}

export function changeBlocks(diff: Pick<FileDiffMetadata, 'hunks'>): ChangeBlock[] {
  const blocks: ChangeBlock[] = [];
  for (const hunk of diff.hunks) {
    let oldStart = hunk.deletionCount ? Math.max(0, hunk.deletionStart - 1) : hunk.deletionStart;
    let newStart = hunk.additionCount ? Math.max(0, hunk.additionStart - 1) : hunk.additionStart;
    for (const content of hunk.hunkContent) {
      if (content.type === 'context') { oldStart += content.lines; newStart += content.lines; }
      else {
        const oldCount = content.deletions;
        const newCount = content.additions;
        if (oldCount || newCount) blocks.push({
          key: `${oldStart}:${oldCount}:${newStart}:${newCount}`,
          oldStart, oldCount, newStart, newCount,
          oldIndex: content.deletionLineIndex, newIndex: content.additionLineIndex,
        });
        oldStart += oldCount;
        newStart += newCount;
      }
    }
  }
  return blocks;
}

/** Visible rows can locate their change in logarithmic time, even in huge diffs. */
export function blockForLine(blocks: ChangeBlock[], line: number, side: SelectionSide): ChangeBlock | undefined {
  const start = side === 'deletions' ? 'oldStart' : 'newStart';
  const count = side === 'deletions' ? 'oldCount' : 'newCount';
  let low = 0;
  let high = blocks.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (blocks[middle][start] <= line - 1) low = middle + 1;
    else high = middle;
  }
  const block = blocks[low - 1];
  return block && block[count] > 0 && line - 1 < block[start] + block[count] ? block : undefined;
}

function lineBoundary(text: string, line: number): number {
  if (!Number.isInteger(line) || line < 0) throw new Error('Bloque de cambios inválido.');
  let offset = 0;
  for (let index = 0; index < line; index++) {
    const newline = text.indexOf('\n', offset);
    if (newline < 0) {
      if (index === line - 1 && offset < text.length) return text.length;
      throw new Error('El bloque ya no coincide con el archivo. Actualizá el diff.');
    }
    offset = newline + 1;
  }
  return offset;
}

export function positionAt(text: string, offset: number): Position {
  let line = 0;
  let start = 0;
  for (let newline = text.indexOf('\n'); newline >= 0 && newline < offset; newline = text.indexOf('\n', start)) {
    line++;
    start = newline + 1;
  }
  return { line, character: offset - start };
}

/** Replace only this block. Preserve all surrounding bytes and EOF/EOL semantics. */
export function restoreBlockEdit(block: ChangeBlock, oldText: string, newText: string, expectedNew: string): TextEdit {
  const start = lineBoundary(newText, block.newStart);
  const end = lineBoundary(newText, block.newStart + block.newCount);
  if (newText.slice(start, end) !== expectedNew) throw new Error('El bloque cambió. Revisá la versión nueva antes de restaurarlo.');
  const oldStart = lineBoundary(oldText, block.oldStart);
  const oldEnd = lineBoundary(oldText, block.oldStart + block.oldCount);
  return { range: { start: positionAt(newText, start), end: positionAt(newText, end) }, newText: oldText.slice(oldStart, oldEnd) };
}

export function restoreFileEdit(oldText: string, newText: string): TextEdit {
  return { range: { start: { line: 0, character: 0 }, end: positionAt(newText, newText.length) }, newText: oldText };
}

export function newBlockText(block: ChangeBlock, text: string): string {
  return text.slice(lineBoundary(text, block.newStart), lineBoundary(text, block.newStart + block.newCount));
}
