// Extraction metadata is public; the text bytes come only from the user's local files.
import manifest from './content-manifest.json' with { type: 'json' };

export const CONTENT_FILE = 'ENGINE-CONTENT.WSR';
export const contentSources = Object.keys(manifest.sources);
const magic = new TextEncoder().encode('WSRCP002');
const required = new Set(manifest.entries.map(entry => entry.id));
const widths = new Map(manifest.entries.filter(entry => entry.encoding).map(entry =>
  [entry.id, entry.encoding === 'f64' ? 8 : 4]));
const maxTextBytes = 65536;
const maxPackBytes = 32 * 1024 * 1024;
const fail = message => { throw new Error(message); };

export function packContent(entries) {
  const texts = new Map();
  for (const { id, bytes } of entries) {
    if (!required.has(id) || texts.has(id)) fail('Unknown or duplicate content identifier');
    if (!(bytes instanceof Uint8Array) || !bytes.length || bytes.length > maxTextBytes ||
        (widths.has(id) && widths.get(id) !== bytes.length))
      fail('Content text has an invalid length');
    texts.set(id, bytes);
  }
  if (texts.size !== required.size) fail('Incomplete content pack');
  const size = 16 + [...texts.values()].reduce((n, value) => n + 8 + value.length, 0);
  if (size > maxPackBytes) fail('Content pack is too large');
  const bytes = new Uint8Array(size);
  const view = new DataView(bytes.buffer);
  bytes.set(magic); view.setUint32(8, texts.size, true);
  let p = 12;
  for (const [id, text] of [...texts].sort(([a], [b]) => a - b)) {
    view.setUint32(p, id, true); view.setUint32(p + 4, text.length, true);
    bytes.set(text, p + 8); p += 8 + text.length;
  }
  view.setUint32(p, crc32(bytes.subarray(0, p)), true);
  return bytes;
}

export function readContentPack(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length < 16 || bytes.length > maxPackBytes)
    fail('Invalid content pack size');
  if (!magic.every((b, i) => bytes[i] === b)) fail('Unsupported content pack version');
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (crc32(bytes.subarray(0, -4)) !== view.getUint32(bytes.length - 4, true)) fail('Content pack checksum mismatch');
  if (view.getUint32(8, true) !== required.size) fail('Unexpected content entry count');
  let p = 12;
  const entries = new Map();
  for (let i = 0; i < required.size; i++) {
    if (p + 8 > bytes.length - 4) fail('Truncated content pack');
    const id = view.getUint32(p, true), length = view.getUint32(p + 4, true);
    if (!required.has(id) || entries.has(id)) fail('Unknown or duplicate content identifier');
    if (!length || length > maxTextBytes || p + 8 + length > bytes.length - 4 ||
        (widths.has(id) && widths.get(id) !== length))
      fail('Content text has an invalid length');
    entries.set(id, bytes.slice(p + 8, p + 8 + length)); p += 8 + length;
  }
  if (p !== bytes.length - 4) fail('Trailing content pack bytes');
  return entries;
}

export async function extractContent(files) {
  for (const [name, expected] of Object.entries(manifest.sources)) {
    const bytes = files.get(name);
    if (!(bytes instanceof Uint8Array)) fail('Missing content source: ' + name);
    const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
    const hash = [...digest].map(b => b.toString(16).padStart(2, '0')).join('');
    if (hash !== expected) fail('Unsupported content source version: ' + name);
  }
  return packContent(manifest.entries.map(entry => {
    const parts = (entry.parts || [entry]).map(part => {
      if (part.token !== undefined) return new TextEncoder().encode(part.token);
      const bytes = files.get(part.source);
      if (part.offset < 0 || part.length < 1 || part.offset + part.length > bytes.length)
        fail('Invalid content extraction range');
      return bytes.subarray(part.offset, part.offset + part.length);
    });
    const bytes = new Uint8Array(parts.reduce((n, part) => n + part.length, 0));
    let offset = 0;
    for (const part of parts) { bytes.set(part, offset); offset += part.length; }
    return { id: entry.id, bytes };
  }));
}

export function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) {
    value ^= byte;
    for (let i = 0; i < 8; i++) value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0);
  }
  return (value ^ 0xffffffff) >>> 0;
}
