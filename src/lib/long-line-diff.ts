import { diffChars } from 'diff';
import type { Change } from 'diff';

// Bound token allocation, edit-graph work and output spans independently of
// the full line length (which is already bounded by the native content limit).
const MAX_CHANGED_LENGTH = 32_768;
const MAX_EDIT_LENGTH = 256;
const TIMEOUT_MS = 20;

/** Character-level emphasis for long lines, including minified source/maps. */
export function diffLongLine(oldLine: string, newLine: string): Change[] {
  const length = Math.min(oldLine.length, newLine.length);
  let start = 0;
  while (start < length && oldLine.charCodeAt(start) === newLine.charCodeAt(start)) start++;
  // Do not split a Unicode code point at either trimmed boundary.
  if (start > 0 && isHighSurrogate(oldLine.charCodeAt(start - 1))) start--;
  let oldEnd = oldLine.length;
  let newEnd = newLine.length;
  while (oldEnd > start && newEnd > start && oldLine.charCodeAt(oldEnd - 1) === newLine.charCodeAt(newEnd - 1)) {
    oldEnd--;
    newEnd--;
  }
  if (oldEnd < oldLine.length && isLowSurrogate(oldLine.charCodeAt(oldEnd))) {
    oldEnd++;
    newEnd++;
  }
  const oldMiddle = oldLine.slice(start, oldEnd);
  const newMiddle = newLine.slice(start, newEnd);
  const detailed = oldMiddle.length + newMiddle.length <= MAX_CHANGED_LENGTH
    ? diffChars(oldMiddle, newMiddle, { timeout: TIMEOUT_MS, maxEditLength: MAX_EDIT_LENGTH })
    : undefined;
  // If refinement exceeds the budget, still exclude the identical ends.
  // Never turn an incomplete character comparison into "unchanged".
  return [
    ...part(oldLine.slice(0, start)),
    ...(detailed ?? [...part(oldMiddle, 'removed'), ...part(newMiddle, 'added')]),
    ...part(oldLine.slice(oldEnd)),
  ];
}

function part(value: string, type?: 'added' | 'removed'): Change[] {
  return value ? [{ value, count: value.length, added: type === 'added', removed: type === 'removed' }] : [];
}
function isHighSurrogate(code: number) { return code >= 0xd800 && code <= 0xdbff; }
function isLowSurrogate(code: number) { return code >= 0xdc00 && code <= 0xdfff; }
