import { describe, expect, it } from 'vitest';
import { hydratePartialDiff, parsePatchFiles } from '@pierre/diffs';
import { changeTargets, completeDiff, contentsLinesText, nativeSelectionText, selectedLinesText, selectionFromContents, validateLoadedContents } from './diff-view';
import { changeBlocks, newBlockText, restoreBlockEdit } from './diff-actions';

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

describe('optional whitespace comparison', () => {
  const versions = (old: string, next: string) => ({
    oldFile: { name: 'sample.es6', contents: old, cacheKey: 'old' },
    newFile: { name: 'sample.es6', contents: next, cacheKey: 'new' },
  });
  it('ignores indentation, tabs, internal/trailing spaces and CRLF, preserving each side for copy', () => {
    const contents = versions('  call ( x );  \r\n\tend();\r\n', 'call(x);\n    end();\n');
    const diff = completeDiff(null, contents, true);
    expect(changeTargets(diff)).toEqual([]);
    expect(diff.splitLineCount).toBe(2);
    expect(selectedLinesText(diff, { start: 1, end: 2, side: 'deletions' })).toBe(contents.oldFile.contents);
    expect(selectedLinesText(diff, { start: 1, end: 2, side: 'additions' })).toBe(contents.newFile.contents);
    expect(completeDiff(null, contents).cacheKey).not.toBe(diff.cacheKey);
    expect(changeTargets(completeDiff(null, contents)).length).toBeGreaterThan(0);
  });
  it('retains real edits and distinct raw context when both types share a hunk', () => {
    const contents = versions('  before\n  value = 1;\n  after\n', 'before\nvalue = 2;\nafter\n');
    const diff = completeDiff(null, contents, true);
    const blocks = changeBlocks(diff);
    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({ oldStart: 1, oldCount: 1, newStart: 1, newCount: 1 });
    expect(diff.deletionLines.join('')).toBe(contents.oldFile.contents);
    expect(diff.additionLines.join('')).toBe(contents.newFile.contents);
    expect(selectedLinesText(diff, { start: 1, end: 1, side: 'deletions' })).toBe('  before\n');
  });
  it('keeps blank-line insertions, line boundaries, final newlines and Unicode spaces significant', () => {
    for (const [old, next] of [['a\nb\n', 'a\n\nb\n'], ['a b\n', 'a\nb\n'], ['a\n', 'a'], ['a\u00a0b\n', 'ab\n']]) {
      expect(changeTargets(completeDiff(null, versions(old, next), true)).length).toBeGreaterThan(0);
    }
  });
  it('does not hide added/deleted files made only of whitespace', () => {
    const file = { name: 'spaces.es6', contents: ' \t\n', cacheKey: 'spaces' };
    const added = completeDiff(null, { oldFile: null, newFile: file }, true);
    const deleted = completeDiff(null, { oldFile: file, newFile: null }, true);
    expect(added.type).toBe('new');
    expect(deleted.type).toBe('deleted');
    expect(changeTargets(added)).toHaveLength(1);
    expect(changeTargets(deleted)).toHaveLength(1);
  });
  it('restores the selected filtered block without touching hidden whitespace elsewhere', () => {
    const contents = versions('  before\n  old();\n  after\n', 'before\nnew();\nafter\n');
    const block = changeBlocks(completeDiff(null, contents, true))[0];
    const edit = restoreBlockEdit(block, contents.oldFile.contents, contents.newFile.contents, newBlockText(block, contents.newFile.contents));
    expect(edit).toBeDefined();
    // TextEdit line boundaries and replacement stay anchored to original bytes.
    expect(edit.newText).toBe('  old();\n');
    expect(edit.range.start.line).toBe(1);
    expect(edit.range.end.line).toBe(2);
  });
  it('recomputes filtered alignment rather than reusing a whitespace-heavy Git patch', () => {
    const exact = completeDiff(null, versions(' old\n', 'old\n'));
    expect(changeTargets(exact)).toHaveLength(1);
    const diff = completeDiff(exact, versions(' old\n', 'old\n'), true);
    expect(changeTargets(diff)).toEqual([]);
    expect(changeTargets(exact)).toHaveLength(1);
  });
  it('handles long lines without normalizing the displayed or copied contents', () => {
    const old = ' '.repeat(80_000) + 'const value = 1;\n';
    const next = '\tconst value=1;\t\n';
    const diff = completeDiff(null, versions(old, next), true);
    expect(changeTargets(diff)).toEqual([]);
    expect(diff.deletionLines.join('')).toBe(old);
    expect(diff.additionLines.join('')).toBe(next);
  });
});

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
