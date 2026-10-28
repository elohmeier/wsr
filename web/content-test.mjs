import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import manifest from './content-manifest.json' with { type: 'json' };
import { packContent, readContentPack, extractContent, contentSources, crc32 } from './content.mjs';

const entries = manifest.entries.map(({ id, encoding }) => ({ id, bytes: encoding
  ? new Uint8Array(encoding === 'f64' ? 8 : 4) : new Uint8Array([0x80, 0xe9, 0, 13, 10]) }));
const packed = packContent(entries);
assert.equal(new TextDecoder().decode(packed.slice(0, 8)), 'WSRCP002');
assert.equal(new DataView(packed.buffer).getUint32(8, true), entries.length);
const read = readContentPack(packed);
for (const entry of entries) assert.deepEqual(read.get(entry.id), entry.bytes);
assert.deepEqual(packContent(entries.toReversed()), packed);
for (const n of [0, 7, 11, packed.length - 1]) assert.throws(() => readContentPack(packed.slice(0, n)));
assert.throws(() => packContent(entries.slice(1)));
assert.throws(() => packContent([...entries, entries[0]]));
for (const [offset, value] of [[8, entries.length + 1], [12, 999], [16, 0xffffffff], [25, 1]]) {
  const invalid = packed.slice(); new DataView(invalid.buffer).setUint32(offset, value, true);
  new DataView(invalid.buffer).setUint32(invalid.length - 4, crc32(invalid.subarray(0, -4)), true);
  assert.throws(() => readContentPack(invalid));
}
const corrupt = packed.slice(); corrupt[20] ^= 1;
assert.throws(() => readContentPack(corrupt), /checksum/);
assert.throws(() => readContentPack(new Uint8Array([...packed, 0])));
await assert.rejects(extractContent(new Map()), /Missing content source/);
console.log('Content protocol: byte preservation, canonical IDs, complete schema and malformed-input rejection passed.');

const [directory, wasm] = process.argv.slice(2);
if (directory) {
  const names = new Map(readdirSync(directory).map(name => [name.toUpperCase(), name]));
  const sources = new Map(contentSources.map(name => [name, new Uint8Array(readFileSync(resolve(directory, names.get(name))))]));
  const actual = readContentPack(await extractContent(sources));
  // Independently read the DLL's counted, NUL-separated table, without the range manifest.
  const dll = sources.get('WSRDLL.DLL'), view = new DataView(dll.buffer, dll.byteOffset, dll.byteLength);
  assert.equal(view.getUint16(0x4a9, true), 27);
  let p = 0x4ab;
  for (let i = 0; i < 27; i++) {
    const end = dll.indexOf(0, p); assert.ok(end > p);
    assert.deepEqual(actual.get(100 + i), dll.slice(p, end)); p = end + 1;
  }
  const mutated = new Map(sources), bad = sources.get('WSRDLL.DLL').slice(); bad[0x4ab] ^= 1;
  mutated.set('WSRDLL.DLL', bad);
  await assert.rejects(extractContent(mutated), /Unsupported content source version/);
  if (wasm) {
    const image = readFileSync(wasm);
    // Short common terms, punctuation and protocol tokens can also occur in independently
    // authored code. Audit long prose here; tools/audit_content.py checks source and artifacts.
    for (const [id, bytes] of actual) if (bytes.length >= 40 && /[a-zA-Z]+ [a-zA-Z]+ [a-zA-Z]+/.test(new TextDecoder('windows-1252').decode(bytes)))
      assert.equal(image.indexOf(bytes), -1, 'Migrated text remains in WASM: ID ' + id);
  }
  console.log('Original content: all 27 DLL records match independently; unknown binary versions rejected' +
    (wasm ? '; migrated long prose absent from built WASM.' : '.'));
}
