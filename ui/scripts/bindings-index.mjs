// Writes src/lib/protocol/index.ts re-exporting every generated type.
import { readdirSync, writeFileSync } from 'node:fs';

const dir = new URL('../src/lib/protocol/', import.meta.url);
const names = readdirSync(dir)
  .filter((f) => f.endsWith('.ts') && f !== 'index.ts')
  .map((f) => f.slice(0, -3))
  .sort();
writeFileSync(
  new URL('index.ts', dir),
  '// Generated re-exports of the Rust protocol types (see `npm run bindings`).\n' +
    names.map((n) => `export type { ${n} } from "./${n}";\n`).join(''),
);
