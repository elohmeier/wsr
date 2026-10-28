// Include license texts from the locked build dependency tree, including bundled fonts.
import { copyFileSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = dirname(fileURLToPath(import.meta.url));
const lock = JSON.parse(readFileSync(join(root, 'package-lock.json'), 'utf8'));
const notices = ['Third-party dependency notices',
  'Project contributions are offered under MIT; see LICENSE. The dependency licenses below do not grant rights in the original game or user-imported resources.'];
for (const [path, entry] of Object.entries(lock.packages).sort(([a], [b]) => a.localeCompare(b))) {
  if (!path) continue;
  const directory = resolve(root, path);
  let names;
  try { names = readdirSync(directory); } catch { continue; } // Optional packages for other platforms.
  const metadata = JSON.parse(readFileSync(join(directory, 'package.json'), 'utf8'));
  notices.push(`\n${metadata.name}@${metadata.version} (${entry.license || metadata.license || 'see package'})`);
  for (const name of names.filter(name => /^(licen[cs]e|copying|notice)([.-]|$)/i.test(name)).sort())
    notices.push(readFileSync(join(directory, name), 'utf8'));
}
// Rust dependencies such as serde_json are linked into the distributed WASM too.
const cargo = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--format-version', '1',
  '--manifest-path', resolve(root, '../engine/Cargo.toml')], { encoding: 'utf8' }));
const workspace = new Set(cargo.workspace_members);
for (const pkg of cargo.packages.filter(pkg => !workspace.has(pkg.id)).sort((a, b) => a.name.localeCompare(b.name))) {
  notices.push(`\nRust: ${pkg.name}@${pkg.version} (${pkg.license || 'see package'})`);
  const directory = dirname(pkg.manifest_path);
  for (const name of readdirSync(directory).filter(name => /^(licen[cs]e|copying|notice)([.-]|$)/i.test(name)).sort())
    notices.push(readFileSync(join(directory, name), 'utf8'));
}
writeFileSync(join(root, 'dist/THIRD-PARTY-NOTICES.txt'), notices.join('\n\n') + '\n');
copyFileSync(resolve(root, '../LICENSE'), join(root, 'dist/LICENSE'));
