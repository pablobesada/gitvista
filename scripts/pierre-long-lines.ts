import { fileURLToPath } from 'node:url';
import type { Plugin } from 'vite';

const helper = fileURLToPath(new URL('../src/lib/long-line-diff.ts', import.meta.url));
const original = 'if (deletionLine.length > maxLineDiffLength || additionLine.length > maxLineDiffLength) return;';

/**
 * Pierre 1.5.2 has no hook for inline diff calculation. Adapt only its long-line
 * branch in both the local renderer and the bundled worker; all decorations,
 * copying, editing and virtualization stay owned by Pierre. Fail on API drift.
 */
export function pierreLongLines(): Plugin {
  return {
    name: 'gitvista-pierre-long-lines',
    enforce: 'pre',
    transform(code, id) {
      const [path, query] = id.split('?');
      // Vite's ?worker module is a constructor wrapper, not the worker source.
      const params = new URLSearchParams(query);
      if (params.has('worker') || params.has('raw') || params.has('url')) return;
      if (!/\/@pierre\/diffs\/dist\/(utils\/renderDiffWithHighlighter|worker\/worker)\.js$/.test(path)) return;
      const calculation = 'const lineDiff = lineDiffType === "char" ? diffChars(deletionLine, additionLine) : diffWordsWithSpace(deletionLine, additionLine);';
      if (!code.includes(original) || !code.includes(calculation)) {
        throw new Error('Pierre inline diff changed; review the bounded long-line adapter before upgrading.');
      }
      return {
        code: `import { diffLongLine } from ${JSON.stringify(helper)};\n` + code
          .replace(original, '')
          .replace(calculation, `const longLine = deletionLine.length > maxLineDiffLength || additionLine.length > maxLineDiffLength;
  const lineDiff = longLine ? diffLongLine(deletionLine, additionLine) : (lineDiffType === "char" ? diffChars(deletionLine, additionLine) : diffWordsWithSpace(deletionLine, additionLine));`)
          // Joining a one-character unchanged gap hides precision in JSON/maps.
          .replace('const enableJoin = lineDiffType === "word-alt";', 'const enableJoin = !longLine && lineDiffType === "word-alt";'),
        map: null,
      };
    },
  };
}
