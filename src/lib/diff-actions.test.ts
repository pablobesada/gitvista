import { describe, expect, it } from 'vitest';
import { parseDiffFromFile, parsePatchFiles } from '@pierre/diffs';
import { TextDocument } from '@pierre/diffs/edit';
import { blockForLine, changeBlocks, newBlockText, restoreBlockEdit, restoreFileEdit } from './diff-actions';
import { changeTargets } from './diff-view';

function diff(oldText: string, newText: string) {
  return parseDiffFromFile({ name: 'file', contents: oldText }, { name: 'file', contents: newText });
}
function restored(oldText: string, newText: string, index = 0) {
  const block = changeBlocks(diff(oldText, newText))[index];
  const document = new TextDocument('fixture', newText);
  document.applyEdits([restoreBlockEdit(block, oldText, newText, newBlockText(block, newText))]);
  return document.getText();
}

describe('restore changes in the current-file draft', () => {
  it('restores only the consecutive block, preserving another change in the same Git hunk', () => {
    const old = 'start\nold A\ncontext\nold B\nend\n';
    const current = 'start\nnew A\ncontext\nnew B\nend\n';
    const model = diff(old, current);
    expect(model.hunks).toHaveLength(1);
    expect(changeBlocks(model)).toHaveLength(2);
    expect(changeTargets(model)).toEqual([{ lineNumber: 2, side: 'additions' }, { lineNumber: 4, side: 'additions' }]);
    expect(restored(old, current)).toBe('start\nold A\ncontext\nnew B\nend\n');
    expect(restored(old, current, 1)).toBe('start\nnew A\ncontext\nold B\nend\n');
  });

  it.each([
    ['addition at start', 'keep\n', 'added\nkeep\n'],
    ['deletion at start', 'removed\nkeep\n', 'keep\n'],
    ['addition at EOF', 'keep\n', 'keep\nadded'],
    ['deletion at EOF', 'keep\nremoved', 'keep\n'],
    ['empty current document', 'original', ''],
    ['empty old document', '', 'added\n'],
    ['CRLF and Unicode', 'á𐐀\r\nold\r\n', 'á𐐀\r\nnew\r\n'],
    ['no final newline', 'keep\nold', 'keep\nnew'],
    ['only final newline', 'same', 'same\n'],
  ])('handles %s without changing unrelated bytes', (_name, old, current) => {
    expect(restored(old, current)).toBe(old);
  });

  it('uses native Git patch coordinates even when context was omitted', () => {
    const old = 'first\nold\nlast\n';
    const current = 'first\nnew\nlast\n';
    const partial = parsePatchFiles('--- a/file\n+++ b/file\n@@ -2 +2 @@\n-old\n+new\n', undefined, true)[0].files[0];
    const block = changeBlocks(partial)[0];
    const document = new TextDocument('fixture', current);
    document.applyEdits([restoreBlockEdit(block, old, current, 'new\n')]);
    expect(document.getText()).toBe(old);
    expect(block.newStart).toBe(1);
    expect(block.newIndex).toBe(0);
  });

  it('rejects a changed or invalid block before applying an edit', () => {
    const block = changeBlocks(diff('old\n', 'new\n'))[0];
    expect(() => restoreBlockEdit(block, 'old\n', 'external\n', 'new\n')).toThrow('El bloque cambió');
    expect(() => restoreBlockEdit({ ...block, newStart: 20 }, 'old\n', 'new\n', 'new\n')).toThrow();
  });

  it('locates only changed rows, with zero-length sides and context excluded', () => {
    const blocks = changeBlocks(diff('first\nremoved\nlast\n', 'first\nlast\nadded\n'));
    expect(blockForLine(blocks, 1, 'additions')).toBeUndefined();
    expect(blockForLine(blocks, 2, 'deletions')).toEqual(blocks[0]);
    expect(blockForLine(blocks, 2, 'additions')).toBeUndefined();
    expect(blockForLine(blocks, 3, 'additions')).toEqual(blocks[1]);
  });

  it('restores the whole file as one undoable edit, including mixed EOL and EOF', () => {
    const old = 'old\r\noriginal';
    const current = 'new\ncurrent\n';
    const document = new TextDocument('fixture', current);
    document.applyEdits([restoreFileEdit(old, current)]);
    expect(document.getText()).toBe(old);
    document.undo();
    expect(document.getText()).toBe(current);
    document.redo();
    expect(document.getText()).toBe(old);
  });

  it('keeps consecutive restorations in separate undo steps', () => {
    const old = 'one\ncontext\ntwo\n';
    const document = new TextDocument('fixture', 'context\n');
    for (let index = 0; index < 2; index++) {
      const current = document.getText();
      const block = changeBlocks(diff(old, current))[0];
      const previous = document.history.undoStack.at(-1);
      if (previous) previous.undoBoundary = true;
      document.applyEdits([restoreBlockEdit(block, old, current, newBlockText(block, current))]);
      document.history.undoStack.at(-1)!.undoBoundary = true;
    }
    expect(document.getText()).toBe(old);
    document.undo();
    expect(document.getText()).toBe('one\ncontext\n');
    document.undo();
    expect(document.getText()).toBe('context\n');
  });
});
