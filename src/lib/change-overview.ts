import type { FileDiffMetadata } from '@pierre/diffs';

export interface OverviewMark {
  row: number;
  count: number;
  side: 'deletions' | 'additions';
  block: number;
}

/** Full split coordinates include unchanged gaps and padding on the shorter side. */
export function changeOverview(hunks: FileDiffMetadata['hunks'], oldLines: number, newLines: number) {
  const marks: OverviewMark[] = [];
  let block = 0;
  for (const hunk of hunks) {
    let row = hunk.splitLineStart;
    for (const content of hunk.hunkContent) {
      if (content.type === 'context') row += content.lines;
      else {
        if (content.deletions) marks.push({ row, count: content.deletions, side: 'deletions', block });
        if (content.additions) marks.push({ row, count: content.additions, side: 'additions', block });
        if (content.deletions || content.additions) block++;
        row += Math.max(content.deletions, content.additions);
      }
    }
  }
  const last = hunks.at(-1);
  const end = (start: number, count: number) => count ? start - 1 + count : start;
  const tail = last ? Math.max(0, oldLines - end(last.deletionStart, last.deletionCount), newLines - end(last.additionStart, last.additionCount)) : 0;
  const rows = Math.max(1, last ? last.splitLineStart + last.splitLineCount + tail : Math.max(oldLines, newLines));
  return { marks, rows };
}

/** Minimum-size thumb still reaches both ends of the ruler. */
export function overviewThumb(scrollTop: number, scrollHeight: number, viewportHeight: number, trackHeight: number) {
  const height = Math.min(trackHeight, Math.max(18, trackHeight * viewportHeight / Math.max(1, scrollHeight)));
  const progress = Math.max(0, Math.min(1, scrollTop / Math.max(1, scrollHeight - viewportHeight)));
  return { top: progress * (trackHeight - height), height };
}
