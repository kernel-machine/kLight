// Copies the release engine and supervisor next to the Tauri shell with the
// target-triple suffix Tauri expects for `externalBin`.
import { execSync } from 'node:child_process';
import { copyFileSync, mkdirSync, chmodSync } from 'node:fs';
import { join } from 'node:path';

const root = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const host = /host: (\S+)/.exec(execSync('rustc -vV').toString())?.[1];
if (!host) throw new Error('cannot determine the rustc host triple');
const ext = process.platform === 'win32' ? '.exe' : '';
const out = join(root, 'src-tauri', 'binaries');
mkdirSync(out, { recursive: true });
for (const name of ['klight-engine', 'klight-supervisor']) {
  const from = join(root, 'target', 'release', name + ext);
  const to = join(out, `${name}-${host}${ext}`);
  copyFileSync(from, to);
  if (!ext) chmodSync(to, 0o755);
  console.log(`${from} -> ${to}`);
}
