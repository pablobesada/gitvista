import { describe, expect, it } from 'vitest';
import { hydratePartialDiff, parsePatchFiles } from '@pierre/diffs';
import { changeTargets, completeDiff, contentsLinesText, nativeSelectionText, selectedLinesText, selectionFromContents, validateLoadedContents } from './diff-view';

const patch = `diff --git a/sample.es6 b/sample.es6
index aaaaaaa..bbbbbbb 100644
--- a/sample.es6
+++ b/sample.es6
@@ -2,3 +2,4 @@
 \tcontext
-old
+new
+
 end
@@ -8,2 +9,2 @@
-last
+replacement
 tail
`;
const model = () => parsePatchFiles(patch, 'fixture', true)[0].files[0];

describe('complete file versions', () => {
  it('shows both full versions even for a net-empty comparison', () => {
    const file = { name: 'sample.es6', contents: 'first\r\nlast\r\n', cacheKey: 'empty' };
    const diff = completeDiff(null, { oldFile: file, newFile: file });
    expect(diff.isPartial).toBeFalsy();
    expect(diff.deletionLines.join('')).toBe(file.contents);
    expect(diff.additionLines.join('')).toBe(file.contents);
    expect(diff.splitLineCount).toBeGreaterThan(0);
    expect(diff.hunks.length).toBeGreaterThan(0);
    expect(changeTargets(diff)).toEqual([]);
  });
  it('shows both sides of a pure rename as a split complete comparison', () => {
    const diff = completeDiff(null, { oldFile: { name: 'old.es6', contents: 'same\n', cacheKey: 'old' }, newFile: { name: 'new.es6', contents: 'same\n', cacheKey: 'new' } });
    expect(diff.type).not.toBe('rename-pure');
    expect(diff.deletionLines.join('')).toBe('same\n');
    expect(diff.additionLines.join('')).toBe('same\n');
  });
  it('retains the absent side for additions and deletions', () => {
    const file = { name: 'sample.es6', contents: 'content\n', cacheKey: 'fixture' };
    const added = completeDiff(null, { oldFile: null, newFile: file });
    const deleted = completeDiff(null, { oldFile: file, newFile: null });
    expect(added.type).toBe('new');
    expect(added.additionLines.join('')).toBe(file.contents);
    expect(deleted.type).toBe('deleted');
    expect(deleted.deletionLines.join('')).toBe(file.contents);
  });
  it('rejects a comparison with neither side available', () => {
    expect(() => completeDiff(null, { oldFile: null, newFile: null })).toThrow();
  });
  it('gives empty documents a renderable row without changing their bytes', () => {
    const empty = { name: 'empty.es6', contents: '', cacheKey: 'empty-document' };
    const diff = completeDiff(null, { oldFile: empty, newFile: empty });
    expect(diff.splitLineCount).toBe(1);
    expect(diff.deletionLines.join('')).toBe('');
    expect(diff.additionLines.join('')).toBe('');
    expect(changeTargets(diff)).toEqual([]);
  });
  it('retains no-final-newline metadata for identical full versions', () => {
    const file = { name: 'sample.es6', contents: 'no final newline', cacheKey: 'no-eol' };
    const diff = completeDiff(null, { oldFile: file, newFile: file });
    expect(diff.hunks[0].noEOFCRDeletions).toBe(true);
    expect(diff.hunks[0].noEOFCRAdditions).toBe(true);
    expect(diff.additionLines.join('')).toBe(file.contents);
  });
});

describe('copy from the Git diff model', () => {
  it('preserves tabs, blank lines, and the selected side without gutters', () => {
    const diff = model();
    expect(selectedLinesText(diff, { start: 2, end: 5, side: 'additions' })).toBe('\tcontext\nnew\n\nend\n');
    expect(selectedLinesText(diff, { start: 4, end: 2, side: 'deletions' })).toBe('\tcontext\nold\nend\n');
  });

  it('requires lazy full contents when selection crosses omitted context', () => {
    const diff = model();
    expect(selectedLinesText(diff, { start: 3, end: 9, side: 'additions' })).toBeNull();
    expect(selectedLinesText(diff, { start: 3, end: 4, side: 'additions', endSide: 'deletions' })).toBeNull();
  });

  it('hydrates in place, preserving Git hunks and enabling offscreen range copy', () => {
    const diff = model();
    const oldFile = { name: 'sample.es6', contents: 'first\n\tcontext\nold\nend\nfive\nsix\nseven\nlast\ntail\n', cacheKey: 'old' };
    const newFile = { name: 'sample.es6', contents: 'first\n\tcontext\nnew\n\nend\nfive\nsix\nseven\nreplacement\ntail\n', cacheKey: 'new' };
    const originalHunks = diff.hunks.map((hunk) => ({ oldStart: hunk.deletionStart, newStart: hunk.additionStart, changes: hunk.additionLines + hunk.deletionLines }));
    expect(() => validateLoadedContents(diff, { oldFile, newFile })).not.toThrow();
    expect(hydratePartialDiff('merge', diff, { oldFile, newFile })).toBe(diff);
    expect(diff.hunks.map((hunk) => ({ oldStart: hunk.deletionStart, newStart: hunk.additionStart, changes: hunk.additionLines + hunk.deletionLines }))).toEqual(originalHunks);
    expect(selectedLinesText(diff, { start: 3, end: 9, side: 'additions' })).toBe('new\n\nend\nfive\nsix\nseven\nreplacement\n');
    expect(selectionFromContents({ oldFile, newFile }, { start: 3, end: 9, side: 'additions' })).toBe('new\n\nend\nfive\nsix\nseven\nreplacement\n');
  });

  it('rejects context changed after the patch, including changes in omitted gaps', () => {
    const diff = model();
    const oldFile = { name: 'sample.es6', contents: 'first\n\tcontext\nold\nend\nfive\nsix\nseven\nlast\ntail\n', cacheKey: 'old' };
    const newFile = { name: 'sample.es6', contents: 'first\n\tcontext\nnew\n\nend\nfive\nsix\nseven\nreplacement\ntail\n', cacheKey: 'new' };
    expect(() => validateLoadedContents(diff, { oldFile, newFile: { ...newFile, contents: newFile.contents.replace('six', 'CHANGED') } })).toThrow('El archivo cambió');
    expect(() => validateLoadedContents(diff, { oldFile, newFile: { ...newFile, contents: newFile.contents.replace('new', 'CHANGED') } })).toThrow('El archivo cambió');
    expect(() => validateLoadedContents(diff, { oldFile, newFile: { ...newFile, contents: newFile.contents + 'extra\n' } })).toThrow('El archivo cambió');
  });

  it('copies native partial characters on only one side, including reversed selection', () => {
    const diff = model();
    expect(nativeSelectionText(diff, { start: { line: 2, character: 1, side: 'additions' }, end: { line: 3, character: 2, side: 'additions' } })).toBe('context\nne');
    expect(nativeSelectionText(diff, { start: { line: 3, character: 2, side: 'deletions' }, end: { line: 3, character: 0, side: 'deletions' } })).toBe('ol');
    expect(nativeSelectionText(diff, { start: { line: 3, character: 0, side: 'deletions' }, end: { line: 3, character: 2, side: 'additions' } })).toBeNull();
  });

  it('keeps final newline semantics and rejects out-of-range lines', () => {
    expect(contentsLinesText('first\r\n\r\nlast', 2, 3)).toBe('\r\nlast');
    expect(contentsLinesText('first\nlast\n', 3, 3)).toBeNull();
    expect(contentsLinesText('first\nlast', 2, 4)).toBeNull();
    expect(contentsLinesText('', 1, 1)).toBeNull();
  });
});

describe('hunk navigation', () => {
  it('targets the first change and falls back to the old side for deletions', () => {
    expect(changeTargets(model())).toEqual([{ lineNumber: 3, side: 'additions' }, { lineNumber: 9, side: 'additions' }]);
    const deletion = parsePatchFiles('--- a/file\n+++ b/file\n@@ -1,2 +1,1 @@\n same\n-deleted\n', undefined, true)[0].files[0];
    expect(changeTargets(deletion)).toEqual([{ lineNumber: 2, side: 'deletions' }]);
  });
});
