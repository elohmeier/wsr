import { extractBranding } from './branding.mjs';
import { CONTENT_FILE, contentSources, extractContent } from './content.mjs';
// Reader for the Inno Setup 2.0.8 package used by wsr985.exe.
// The file layout follows innoextract (https://github.com/dscharrer/innoextract),
// independently implemented here for this one installer format. No installer code is executed.
const text = new TextDecoder('windows-1252');
const required = ['WSR101.DAT', 'CORPNAME.DAT', 'MSCNEWS.DAT', 'SCORES.WSR'];
const fail = () => { throw new Error('Unable to read this installer. Choose the original wsr985.exe file.'); };
class Reader {
  constructor(bytes, pos = 0) { this.bytes = bytes; this.pos = pos; this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength); }
  take(n) { if (!Number.isSafeInteger(n) || n < 0 || this.pos + n > this.bytes.length) fail(); const b = this.bytes.subarray(this.pos, this.pos + n); this.pos += n; return b; }
  u32() { const p = this.pos; this.take(4); return this.view.getUint32(p, true); }
  string() { const n = this.u32(); if (n > 65536) fail(); return text.decode(this.take(n)); }
}
const crcTable = Uint32Array.from({ length:256 }, (_, n) => {
  for (let k = 0; k < 8; k++) n = (n >>> 1) ^ (n & 1 ? 0xedb88320 : 0);
  return n >>> 0;
});
function crc32(bytes) {
  let n = 0xffffffff;
  for (const b of bytes) n = crcTable[(n ^ b) & 255] ^ (n >>> 8);
  return (n ^ 0xffffffff) >>> 0;
}
function adler32(bytes) {
  let a = 1, b = 0;
  for (const c of bytes) { a = (a + c) % 65521; b = (b + a) % 65521; }
  return ((b << 16) | a) >>> 0;
}
async function inflate(bytes, size) {
  if (size > 64 * 1024 * 1024) fail();
  const reader = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate')).getReader();
  const parts = []; let total = 0;
  try {
    for (;;) {
      const { value, done } = await reader.read(); if (done) break;
      total += value.length;
      if (total > size) { await reader.cancel(); fail(); }
      parts.push(value);
    }
  } finally { reader.releaseLock(); }
  if (total !== size) fail();
  const out = new Uint8Array(total); let p = 0;
  for (const part of parts) { out.set(part, p); p += part.length; }
  return out;
}
async function block(r) {
  const expected = r.u32(), start = r.pos, compressed = r.u32(), size = r.u32();
  if (crc32(r.bytes.subarray(start, r.pos)) !== expected || size > 8 * 1024 * 1024) fail();
  let left = compressed === 0xffffffff ? size : compressed;
  if (left > 8 * 1024 * 1024) fail();
  const packed = new Uint8Array(left); let p = 0;
  while (left) {
    const check = r.u32(), part = r.take(Math.min(left, 4096));
    if (crc32(part) !== check) fail();
    packed.set(part, p); p += part.length; left -= part.length;
  }
  return compressed === 0xffffffff ? packed : inflate(packed, size);
}
function fileTable(header, locations) {
  const hr = new Reader(header);
  if (hr.string() !== 'Wall Street Raider' || hr.string() !== 'Wall Street Raider 9.85') fail();
  for (let i = 2; i < 20; i++) hr.take(hr.u32()); // includes the large installation help text
  hr.take(48); // lead-byte set, type/component/task/directory counts
  const count = hr.u32(), dataCount = hr.u32();
  if (count < required.length || count > 1000 || dataCount !== locations.length) fail();
  // The first user file is WSR.EXE. Locate its length-prefixed destination, then read
  // complete 2.0.8 records in sequence (five strings, WinVersion range, fields and flags).
  const marker = new TextEncoder().encode('{app}\\WSR.EXE');
  let start = -1;
  outer: for (let p = 8; p + marker.length < header.length; p++) {
    for (let k = 0; k < marker.length; k++) if ((header[p + k] | 32) !== (marker[k] | 32)) continue outer;
    const prefix = new Reader(header, p - 8);
    if (prefix.u32() === 0 && prefix.u32() === marker.length) { start = p - 8; break; }
  }
  if (start < 0) fail();
  // An empty 56-byte uninstaller entry precedes the first user file in this package.
  if (start < 56) fail();
  const r = new Reader(header, start - 56), files = [];
  for (let i = 0; i < count; i++) {
    r.string(); const destination = r.string();
    r.string(); r.string(); r.string(); r.take(20);
    const location = r.u32(), fields = r.take(12); // attributes, external size, copy mode, 2 flag bytes, type
    if (location >= locations.length) fail();
    if (!destination && location === 0 && fields[11] === 1) continue;
    if (!destination.toLowerCase().startsWith('{app}\\')) fail();
    const name = destination.slice(6).toUpperCase();
    if (contentSources.includes(name) || /^[A-Z0-9_. -]+\.(DAT|WSR|ORI)$/.test(name) && !/^GAME\d+\.DAT$/.test(name) && name !== 'LASTGAME.DAT' || /^(HELP\.TXT|GLOSSARY\.TXT|WSRBOOK\.HTM|[A-Z]+\.JPG)$/.test(name)) {
      files.push({ name, ...locations[location] });
    }
  }
  if (!required.every((name) => files.some((f) => f.name === name))) fail();
  return files;
}
export async function extractInstaller(bytes, progress = () => {}) {
  try {
    if (!(bytes instanceof Uint8Array) || bytes.length > 32 * 1024 * 1024) fail();
    const r = new Reader(bytes, 0x30);
    if (r.u32() !== 0x6f6e6e49) fail(); // "Inno"
    const table = r.u32(), complement = r.u32();
    if ((table ^ complement) !== -1) fail();
    r.pos = table;
    if (!r.take(12).every((b, i) => b === [114,68,108,80,116,83,48,50,135,101,86,120][i])) fail();
    r.take(24); const headerOffset = r.u32(), dataOffset = r.u32();
    r.pos = headerOffset;
    if (text.decode(r.take(64)).replace(/\0.*$/s, '') !== 'Inno Setup Setup Data (2.0.8)') fail();
    const header = await block(r), data = await block(r);
    if (!data.length || data.length % 41) fail();
    const dr = new Reader(data), locations = [];
    while (dr.pos < data.length) {
      if (dr.u32() !== 1 || dr.u32() !== 1) fail(); // single embedded data slice
      const offset = dr.u32(), size = dr.u32(), packedSize = dr.u32(), checksum = dr.u32();
      dr.take(17); // file time, version, flags (2.0.8 has no encryption or instruction filter)
      locations.push({ offset:dataOffset + offset, size, packedSize, checksum });
    }
    const files = fileTable(header, locations), out = [], sources = new Map();
    if (files.reduce((n, f) => n + f.size, 0) > 128 * 1024 * 1024) fail();
    for (const f of files) {
      progress('Reading ' + f.name + '…');
      const fr = new Reader(bytes, f.offset);
      if (fr.u32() !== 0x1a626c7a) fail(); // "zlb\x1a"
      const content = await inflate(fr.take(f.packedSize), f.size);
      if (adler32(content) !== f.checksum) fail();
      if (contentSources.includes(f.name)) {
        sources.set(f.name, content);
        if (f.name === 'WSR.EXE') out.push(...extractBranding(content));
      } else out.push({ name:f.name, bytes:content });
    }
    out.push({ name: CONTENT_FILE, bytes: await extractContent(sources) });
    return out;
  } catch (e) {
    if (e.message?.startsWith('Unable to read this installer.')) throw e;
    fail();
  }
}
