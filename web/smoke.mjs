// Node smoke test of wsr.wasm: node web/smoke.mjs path/to/GAMExx.DAT
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const stub = () => 0;
const host = {
  host_local_time: (p) => new Int32Array(w.memory.buffer, p, 7).set([2000, 1, 3, 10, 0, 0, 0]),
  host_choose: () => -1, host_chart: () => 0, host_diagram: () => -1, host_option_station: () => 0, host_msgbox: stub, host_inputbox: stub, host_report: stub, host_last_link: stub, host_news: () => {},
};
const { instance } = await WebAssembly.instantiate(readFileSync(new URL('wsr.wasm', import.meta.url)), { host });
const w = instance.exports;
const bytes = readFileSync(process.argv[2]);
const p = w.wsr_alloc(bytes.length);
new Uint8Array(w.memory.buffer, p, bytes.length).set(bytes);
const r = w.wsr_load_save(p, bytes.length, 0);
const len = new DataView(w.memory.buffer).getUint32(r, true);
const res = JSON.parse(new TextDecoder().decode(new Uint8Array(w.memory.buffer, r + 4, len)));
assert.equal(res.ok, true, res.error);
assert.ok(res.summary.players.length > 0, 'Loaded save has no players');
console.log(JSON.stringify({ ok: res.ok, year: res.summary?.year, players: res.summary?.players?.map((x) => x.name), top: res.summary?.topCorps?.[0]?.symbol, error: res.error }));
for (const [kind, ent] of [[2, 0], [3, 0], [1, 131]]) {
  const r = w.wsr_report(kind, ent);
  const n = new DataView(w.memory.buffer).getUint32(r, true);
  const rep = JSON.parse(new TextDecoder().decode(new Uint8Array(w.memory.buffer, r + 4, n)));
  assert.equal(rep.ok, true, rep.error);
  assert.ok(rep.lines.length > 0, `Report ${kind} is empty`);
  console.log(kind, rep.ok, rep.title, rep.lines.length, 'lines;', rep.lines.slice(0, 3).map((l) => l.text).join(' | '), rep.messages.slice(0, 1));
}
for (const [kind, ent] of [[4, 131], [5, 131], [6, 131], [7, 1], [8, 1], [9, 4], [11, 0], [12, 131], [13, 0]]) {
  const r = w.wsr_report(kind, ent);
  const n = new DataView(w.memory.buffer).getUint32(r, true);
  const rep = JSON.parse(new TextDecoder().decode(new Uint8Array(w.memory.buffer, r + 4, n)));
  assert.equal(rep.ok, true, rep.error);
  // An empty portfolio is a valid result for a player with no positions.
  if (kind !== 7) assert.ok(rep.lines.length || rep.messages.length, `Report ${kind} produced no output`);
  console.log(kind, rep.ok, JSON.stringify(rep.title).slice(0, 70), rep.lines.length, 'lines', rep.messages.length, 'msgs');
}
