import type { ChangeEntry, DiffContents, DiffMode, DiffResult, Repository, WorkspaceSnapshot } from './types';
import { createTwoFilesPatch } from 'diff';
const paths = [
  ['.', 'package.json', 'M', 'M'],
  ['.', 'src/server.ts', '.', 'M'],
  ['scriptdirs/src/oppen-server-admin', 'scripts_templates/docker_templates/deployOppen.py', '.', 'M'],
  ['scriptdirs/src/oppen-server-admin', 'server_guard/config.py', '.', 'M'],
  ['scriptdirs/src/oppen-server-admin', 'README.md', 'M', '.'],
  ['scriptdirs/src/base', 'records/Customer.es6', '.', 'M'],
  ['scriptdirs/src/base', 'windows/CustomerWindow.es6', '.', 'M'],
  ['scriptdirs/src/base', 'tools/InvoiceTotals.es6', 'M', '.'],
  ['scriptdirs/src/base', 'test/classes/CustomerTest.es6', '?', '?'],
  ['scriptdirs/src/ar', 'records/Invoice.es6', '.', 'M'],
  ['scriptdirs/src/ar', 'reports/SalesReport.es6', '.', 'M'],
  ['scriptdirs/src/opencustom', 'records/Order.es6', '.', 'M'],
] as const;
export function demoWorkspace(root: string, generation: number): WorkspaceSnapshot {
  const repos = [...new Set(paths.map((p) => p[0]))].map((relativePath): Repository => ({ id: relativePath, path: relativePath === '.' ? root : `${root}/${relativePath}`, relativePath, gitDir: `${root}/${relativePath}/.git`, commonDir: `${root}/${relativePath}/.git`, branch: relativePath === '.' ? 'develop' : 'master' }));
  return { root, generation, scanning: false, scannedCount: repos.length, repositories: repos.map((repository) => ({ repository, elapsedMs: 18, error: null, entries: paths.filter((p) => p[0] === repository.id).map((p): ChangeEntry => ({ id: `${p[0]}:${p[1]}`, repoId: p[0], path: p[1], oldPath: null, absolutePath: `${repository.path}/${p[1]}`, relativePath: p[0] === '.' ? p[1] : `${p[0]}/${p[1]}`, indexStatus: p[2], worktreeStatus: p[3], kind: 'file' })) })) };
}
const oldPython = `import argparse\nfrom pathlib import Path\n\ndef configure_server(args):\n    \"\"\"Prepare the server configuration.\"\"\"\n    configuration = {}\n    configuration["instance"] = args.instance\n    configuration["workers"] = args.workers\n\n    if args.template_backup is not None:\n        if args.skip_database_creation:\n            raise ValueError("template requires database creation")\n        configuration["backup"] = Path(args.template_backup)\n\n    return configuration\n`;
const newPython = oldPython.replace('args.template_backup is not None', 'args.template_backup');
const oldJs = `export class Customer {\n    constructor(code, name) {\n        this.code = code;\n        this.name = name;\n    }\n\n    getDisplayName() {\n        return this.name;\n    }\n\n    isActive() {\n        return true;\n    }\n}\n`;
const newJs = oldJs.replace('return this.name;', 'return `${this.code} — ${this.name}`;');
export function demoDiff(entryId: string, mode: DiffMode): DiffResult {
  const row = paths.find((p) => `${p[0]}:${p[1]}` === entryId) ?? paths[2];
  const path = row[1];
  const isPython = path.endsWith('.py');
  let patch = isPython
    ? `diff --git a/${path} b/${path}\nindex aaaaaaa..bbbbbbb 100644\n--- a/${path}\n+++ b/${path}\n@@ -7,7 +7,7 @@ def configure_server(args):\n     configuration["instance"] = args.instance\n     configuration["workers"] = args.workers\n \n-    if args.template_backup is not None:\n+    if args.template_backup:\n         if args.skip_database_creation:\n             raise ValueError("template requires database creation")\n         configuration["backup"] = Path(args.template_backup)\n`
    : `diff --git a/${path} b/${path}\nindex aaaaaaa..bbbbbbb 100644\n--- a/${path}\n+++ b/${path}\n@@ -5,7 +5,7 @@ export class Customer {\n     }\n \n     getDisplayName() {\n-        return this.name;\n+        return \u0060\u0024{this.code} — \u0024{this.name}\u0060;\n     }\n \n     isActive() {\n`;
  if (drafts.has(entryId)) patch = `diff --git a/${path} b/${path}\n${createTwoFilesPatch(`a/${path}`, `b/${path}`, isPython ? oldPython : oldJs, drafts.get(entryId)!, '', '', { context: 3 })}`;
  return { entryId, repoId: row[0], mode, patch, kind: 'text', message: null, oldPath: path, newPath: path };
}
export function demoContents(entryId: string, _mode: DiffMode): DiffContents {
  const isPython = entryId.endsWith('.py');
  const name = entryId.slice(entryId.indexOf(':') + 1);
  return { oldFile: { name, contents: isPython ? oldPython : oldJs, cacheKey: `demo:${entryId}:old` }, newFile: { name, contents: drafts.get(entryId) ?? (isPython ? newPython : newJs), cacheKey: `demo:${entryId}:new:${drafts.get(entryId) ?? ''}` } };
}
const drafts = new Map<string, string>();
export function saveDemoContents(entryId: string, contents: string) { drafts.set(entryId, contents); }
