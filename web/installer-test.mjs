// node web/installer-test.mjs wsr985.exe extracted/app
// Compares the browser reader with files independently extracted by innoextract.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {extractBranding} from './branding.mjs';
import manifest from './content-manifest.json' with { type: 'json' };
import { CONTENT_FILE, readContentPack } from './content.mjs';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { extractInstaller } from './installer.mjs';
import { parseCompanies, editCompany, serializeCompanies } from './companies.mjs';
import { helpTopics } from './docs.mjs';
const source = new Uint8Array(readFileSync(process.argv[2]));
const directory = process.argv[3];
const names = new Map(readdirSync(directory).map((name) => [name.toUpperCase(), name]));
const files = await extractInstaller(source);
assert.equal(files.length, 26);
const artwork = new Map([
  ['WSR-BANNER.BMP','5c7756304bef3ec441487bf98be311298151639ff3b157b674f57f7bd1c53dde'],
  ['WSR-NEWS.BMP','e71d014094d6336c5e4494486c58fb6bfc96047dc6a7f2411fa78b289310eb6b'],
]);
for (const name of ['HELP.TXT','GLOSSARY.TXT','WSRBOOK.HTM','CORPNAME.ORI','ADVANOPT.JPG']) assert.ok(files.some(f => f.name === name), name);
for (const f of files) {
  assert.ok(!/\.(EXE|DLL)$/i.test(f.name),'Executable code must not be persisted');
  if (f.name === CONTENT_FILE) { assert.equal(readContentPack(f.bytes).size, manifest.entries.length); continue; }
  if(artwork.has(f.name)){assert.equal(createHash('sha256').update(f.bytes).digest('hex'),artwork.get(f.name));continue;}
  assert.ok(names.has(f.name), 'Unknown extracted file: ' + f.name);
  assert.deepEqual(f.bytes, new Uint8Array(readFileSync(join(directory, names.get(f.name)))), f.name);
  assert.ok(!/^GAME\d+\.DAT$/.test(f.name), 'Installer must not import dummy saved-game slots');
}
const companies = parseCompanies(files.find(f => f.name === 'CORPNAME.DAT').bytes);
const customized = editCompany(companies,11,{name:'Browser Test Bank',symbol:'ZTEST',country:16});
assert.deepEqual(parseCompanies(serializeCompanies(customized)),customized);
assert.equal(companies[0].id, 11);
assert.deepEqual(parseCompanies(serializeCompanies(companies)), companies);
for (const edit of [
  {name:'Test Bank',symbol:companies[1].symbol,country:0},
  {name:companies[1].name,symbol:'ZTEST',country:0}, {name:'Test Bank',symbol:'ZTEST',country:54},
  {name:'Unsupported \u{1f4c8}',symbol:'ZTEST',country:0},
]) assert.throws(() => editCompany(companies,11,edit));
assert.throws(() => parseCompanies(files.find(f => f.name === 'CORPNAME.ORI').bytes));
assert.equal(helpTopics(files.find(f => f.name === 'HELP.TXT').bytes).length,192);
for (const bad of [source.slice(0, 100), source.slice(0, source.length / 2), new Uint8Array(100)]) {
  await assert.rejects(extractInstaller(bad), /Choose the original wsr985.exe/);
}
const corrupt = source.slice();
const view = new DataView(corrupt.buffer);
const table = view.getUint32(0x34, true), header = view.getUint32(table + 36, true);
corrupt[header + 64 + 16 + 20] ^= 1; // a CRC-protected setup-header block
await assert.rejects(extractInstaller(corrupt), /Choose the original wsr985.exe/);
for(const length of [0,40,1024])assert.throws(()=>extractBranding(new Uint8Array(length)));
console.log('Installer: 23 files match innoextract and both original bitmap resources match their reference hashes; runtime content is extracted locally; invalid, truncated and corrupt packages rejected.');
