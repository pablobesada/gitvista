import { describe, expect, it } from 'vitest';
import { parseDiffFromFile } from '@pierre/diffs';
import { changeOverview, overviewThumb } from './change-overview';

function overview(old: string, current: string) {
  const diff = parseDiffFromFile({ name: 'file', contents: old }, { name: 'file', contents: current });
  return changeOverview(diff.hunks, diff.deletionLines.length, diff.additionLines.length);
}

describe('full-file change overview', () => {
  it('includes long unchanged gaps and the unchanged tail in the full-file scale', () => {
    const lines = Array.from({ length: 200 }, (_, index) => `line ${index}\n`);
    const next = [...lines];
    next[10] = 'first change\n';
    next[150] = 'second change\n';
    const map = overview(lines.join(''), next.join(''));
    expect(map.rows).toBe(200);
    expect(map.marks).toEqual([
      { row: 10, count: 1, side: 'deletions', block: 0 },
      { row: 10, count: 1, side: 'additions', block: 0 },
      { row: 150, count: 1, side: 'deletions', block: 1 },
      { row: 150, count: 1, side: 'additions', block: 1 },
    ]);
  });

  it('keeps deleted rows in split alignment, including a later addition at EOF', () => {
    const map = overview('first\nremoved A\nremoved B\nkeep\nend\n', 'first\nkeep\nend\nadded\n');
    expect(map.rows).toBe(6);
    expect(map.marks).toEqual([
      { row: 1, count: 2, side: 'deletions', block: 0 },
      { row: 5, count: 1, side: 'additions', block: 1 },
    ]);
  });

  it('has no invented changes for identical or empty files', () => {
    expect(overview('same\n', 'same\n')).toEqual({ rows: 1, marks: [] });
    expect(overview('', '')).toEqual({ rows: 1, marks: [] });
  });

  it.each([
    ['', 'added\n', 'additions'],
    ['removed\n', '', 'deletions'],
  ] as const)('handles one empty side', (old, current, side) => {
    expect(overview(old, current)).toEqual({ rows: 1, marks: [{ row: 0, count: 1, side, block: 0 }] });
  });

  it('keeps a minimum-size thumb within the track at both scroll boundaries', () => {
    expect(overviewThumb(0, 100000, 500, 500)).toEqual({ top: 0, height: 18 });
    expect(overviewThumb(99500, 100000, 500, 500)).toEqual({ top: 482, height: 18 });
    expect(overviewThumb(0, 100, 500, 500)).toEqual({ top: 0, height: 500 });
  });
});
