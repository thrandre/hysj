// `npm run types`: regenerates src/lib/gen from crates/hysj-proto (ts-rs) and
// the index that re-exports it.
import { execSync } from 'node:child_process';
import { readdirSync, writeFileSync } from 'node:fs';

execSync('cargo test -q -p hysj-proto --features ts', { cwd: '../../..', stdio: 'inherit' });
const dir = new URL('../src/lib/gen/', import.meta.url);
const names = readdirSync(dir)
  .filter((f) => f.endsWith('.ts') && f !== 'index.ts')
  .map((f) => f.slice(0, -3))
  .sort();
writeFileSync(new URL('index.ts', dir), names.map((n) => `export type { ${n} } from './${n}';`).join('\n') + '\n');
