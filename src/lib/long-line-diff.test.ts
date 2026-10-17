import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import type { Change } from 'diff';
import { diffLongLine } from './long-line-diff';
import { pierreLongLines } from '../../scripts/pierre-long-lines';

function changes(oldLine: string, newLine: string) {
  const result = diffLongLine(oldLine, newLine);
  expect(result.filter(part => !part.added).map(part => part.value).join('')).toBe(oldLine);
  expect(result.filter(part => !part.removed).map(part => part.value).join('')).toBe(newLine);
  return result;
}
const emphasis = (result: Change[], type: 'added' | 'removed') => result.filter(part => part[type]).map(part => part.value);

describe('bounded long-line emphasis', () => {
  it('finds a tiny edit inside a multi-megabyte source map without tokenizing its identical ends', () => {
    const prefix = '{"mappings":"' + 'AAAA;'.repeat(400_000);
    const suffix = '","sourcesContent":[' + 'unchanged'.repeat(100_000) + ']}';
    const result = changes(prefix + 'old' + suffix, prefix + 'new' + suffix);
    expect(emphasis(result, 'removed')).toEqual(['old']);
    expect(emphasis(result, 'added')).toEqual(['new']);
  });

  it('separates multiple edits and leaves even single-character common gaps unmarked', () => {
    const prefix = ' '.repeat(3_000);
    const result = changes(prefix + '1,2;keep;3', prefix + '4,5;keep;6');
    expect(emphasis(result, 'removed')).toEqual(['1', '2', '3']);
    expect(emphasis(result, 'added')).toEqual(['4', '5', '6']);
  });

  it('handles insertions, deletions and changes at either boundary', () => {
    const same = 'x'.repeat(5_000);
    for (const [oldLine, newLine] of [[same, 'NEW' + same], [same + 'OLD', same], ['OLD' + same, 'NEW' + same], [same + 'OLD', same + 'NEW']]) {
      const result = changes(oldLine, newLine);
      expect(emphasis(result, 'removed').join('')).toBe(oldLine.includes('OLD') ? 'OLD' : '');
      expect(emphasis(result, 'added').join('')).toBe(newLine.includes('NEW') ? 'NEW' : '');
    }
  });

  it('preserves CRLF, tabs and whole Unicode code points in changed spans', () => {
    const prefix = '\t'.repeat(3_000);
    for (const [oldValue, newValue] of [['😀', '😃'], ['😀', '🈀'], ['', '🧉']]) {
      const result = changes(prefix + oldValue + '\r\n', prefix + newValue + '\r\n');
      expect(emphasis(result, 'removed').join('')).toBe(oldValue);
      expect(emphasis(result, 'added').join('')).toBe(newValue);
    }
  });

  it('keeps identical lines and empty strings free of emphasis', () => {
    for (const line of ['', 'same'.repeat(3_000), '😀']) {
      const result = changes(line, line);
      expect(emphasis(result, 'removed')).toEqual([]);
      expect(emphasis(result, 'added')).toEqual([]);
    }
  });

  it('falls back to the differing region when allocation or edit-distance budgets are exceeded', () => {
    for (const size of [2_000, 100_000]) {
      const oldMiddle = 'a'.repeat(size);
      const newMiddle = 'b'.repeat(size);
      const result = changes('prefix:' + oldMiddle + ':suffix', 'prefix:' + newMiddle + ':suffix');
      expect(emphasis(result, 'removed')).toEqual([oldMiddle]);
      expect(emphasis(result, 'added')).toEqual([newMiddle]);
      expect(result).toHaveLength(4);
    }
  });
});

describe('Pierre inline adapter compatibility', () => {
  const plugin = pierreLongLines();
  // Exercise the exact installed files, including the worker's bundled copy.
  const transform = plugin.transform as (code: string, id: string) => { code: string } | undefined;
  it.each(['utils/renderDiffWithHighlighter', 'worker/worker'])('adapts %s without changing the short-line calculation', path => {
    const file = new URL(`../../node_modules/@pierre/diffs/dist/${path}.js`, import.meta.url);
    const adapted = transform(readFileSync(file, 'utf8'), file.pathname)?.code;
    expect(adapted).toContain('longLine ? diffLongLine(deletionLine, additionLine)');
    expect(adapted).toContain('diffWordsWithSpace(deletionLine, additionLine)');
    expect(adapted).toContain('!longLine && lineDiffType === "word-alt"');
  });
  it('fails visibly on upstream drift and leaves other modules alone', () => {
    expect(() => transform('changed upstream', '/node_modules/@pierre/diffs/dist/worker/worker.js')).toThrow('Pierre inline diff changed');
    expect(transform('unrelated', '/src/lib/diff-view.ts')).toBeUndefined();
    expect(transform('worker wrapper', '/node_modules/@pierre/diffs/dist/worker/worker.js?worker')).toBeUndefined();
  });
});
