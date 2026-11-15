// Copies the production build into the daemon's embedded UI folder.
import { cpSync, rmSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const dist = resolve(here, '../dist');
const target = resolve(here, '../../crates/tvmaster-server/ui');
if (!existsSync(dist)) {
  console.error('No dist/ folder: run `npm run build` first.');
  process.exit(1);
}
rmSync(target, { recursive: true, force: true });
cpSync(dist, target, { recursive: true });
console.log(`Copied ${dist} -> ${target}`);
