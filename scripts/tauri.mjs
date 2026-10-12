import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { delimiter, join } from 'node:path';
const cargoBin = join(homedir(), '.cargo', 'bin');
const env = { ...process.env };
if (existsSync(join(cargoBin, process.platform === 'win32' ? 'cargo.exe' : 'cargo'))) {
  env.PATH = `${cargoBin}${delimiter}${env.PATH ?? ''}`;
}
const cli = join(import.meta.dirname, '../node_modules/@tauri-apps/cli/tauri.js');
const child = spawn(process.execPath, [cli, ...process.argv.slice(2)], { env, stdio: 'inherit' });
child.on('exit', (code) => process.exit(code ?? 1));
