// Native test/tool adapter. Generated packs belong in ignored local directories.
import { readFileSync, readdirSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { contentSources, extractContent } from './content.mjs';

const [directory, output] = process.argv.slice(2);
if (!directory || !output) throw new Error('Usage: node web/extract-content.mjs EXTRACTED_APP OUTPUT');
const names = new Map(readdirSync(directory).map(name => [name.toUpperCase(), name]));
const sources = new Map(contentSources.map(name => {
  if (!names.has(name)) throw new Error('Missing content source: ' + name);
  return [name, new Uint8Array(readFileSync(resolve(directory, names.get(name))))];
}));
mkdirSync(dirname(resolve(output)), { recursive: true });
writeFileSync(output, await extractContent(sources));
console.log('Wrote runtime content pack: ' + resolve(output));
