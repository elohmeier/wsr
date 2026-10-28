// node web/engine-benchmark.mjs engine.wasm /path/to/complete/GAME01.DAT extracted/app [ticks]
// Runs the actual WASM engine with deterministic time and noninteractive host replies.
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import assert from 'node:assert/strict';
import { CONTENT_FILE, contentSources, extractContent } from './content.mjs';
const [wasm, save, assets, count = '1000'] = process.argv.slice(2);
if (!wasm || !save || !assets)
	throw Error('Provide WASM, a complete save and extracted assets');
let w,
	clock = Date.UTC(2025, 0, 3, 10, 1, 30, 125);
const enc = new TextEncoder(),
	dec = new TextDecoder();
const allocate = (bytes) => {
	const p = w.wsr_alloc(bytes.length);
	new Uint8Array(w.memory.buffer, p, bytes.length).set(bytes);
	return p;
};
const take = (p) => {
	const n = new DataView(w.memory.buffer).getUint32(p, true);
	const bytes = new Uint8Array(w.memory.buffer, p + 4, n).slice();
	w.wsr_free(p, n + 4);
	return bytes;
};
const json = (p) => JSON.parse(dec.decode(take(p)));
const host = {
	host_local_time: (p) => {
		const t = new Date(clock);
		new Int32Array(w.memory.buffer, p, 7).set([
			t.getUTCFullYear(),
			t.getUTCMonth() + 1,
			t.getUTCDate(),
			t.getUTCHours(),
			t.getUTCMinutes(),
			t.getUTCSeconds(),
			t.getUTCMilliseconds(),
		]);
	},
	host_msgbox: () => 1,
	host_report: () => 1,
	host_choose: () => -1,
	host_chart: () => 0,
	host_diagram: () => 0,
	host_news: () => {},
	host_inputbox: (_p, _n, _t, _tn, d, dn) => {
		const bytes = new Uint8Array(w.memory.buffer, d, dn).slice();
		const p = w.wsr_alloc(dn + 4);
		new DataView(w.memory.buffer).setUint32(p, dn, true);
		new Uint8Array(w.memory.buffer, p + 4, dn).set(bytes);
		return p;
	},
	host_last_link: () => {
		const p = w.wsr_alloc(4);
		new DataView(w.memory.buffer).setUint32(p, 0, true);
		return p;
	},
	host_option_station: () => 0,
};
w = (await WebAssembly.instantiate(readFileSync(wasm), { host })).instance
	.exports;
const names = new Map(readdirSync(assets).map(name => [name.toUpperCase(), name]));
const content = await extractContent(new Map(contentSources.map(name =>
	[name, new Uint8Array(readFileSync(join(assets, names.get(name))))])));
const files = readdirSync(assets).filter(name => /\.(dat|txt)$/i.test(name))
	.map(name => ({ name, bytes: readFileSync(join(assets, name)) }));
files.push({ name: CONTENT_FILE, bytes: content });
for (const { name, bytes } of files) {
	const nb = enc.encode(name);
	const p = allocate(nb),
		q = allocate(bytes);
	const r = json(w.wsr_put_file(p, nb.length, q, bytes.length));
	w.wsr_free(p, nb.length);
	w.wsr_free(q, bytes.length);
	if (!r.ok) throw Error(JSON.stringify(r));
}
const bytes = readFileSync(save),
	p = allocate(bytes),
	start = performance.now();
const loaded = json(w.wsr_open_game(p, bytes.length, 0, 1));
w.wsr_free(p, bytes.length);
if (!loaded.ok) throw Error(JSON.stringify(loaded));
const loadMs = performance.now() - start;
// Desk control 249 toggles the ticker; command 5 runs its timer callback.
const initial = json(w.wsr_command(0, 0, 0, 0));
if (initial.summary?.clock !== 'RUN') json(w.wsr_command(2, 249, 0, 0));
const timings = [];
for (let i = 0; i < Number(count); i++) {
	clock += 2000;
	const t = performance.now();
	const result = json(w.wsr_command(5, 0, 0, 0));
	if (!result.ok) throw Error(JSON.stringify(result));
	timings.push(performance.now() - t);
}
const final = json(w.wsr_command(0, 0, 0, 0));
assert.notEqual(
	final.summary.gameDate,
	initial.summary.gameDate,
	'Benchmark did not advance the simulated date; use at least 1000 ticks',
);
timings.sort((a, b) => a - b);
console.log(
	JSON.stringify(
		{
			wasm,
			ticks: timings.length,
			loadMs,
			medianMs: timings[Math.floor(timings.length / 2)],
			p95Ms: timings[Math.floor(timings.length * 0.95)],
			maxMs: timings.at(-1),
			totalMs: timings.reduce((a, b) => a + b, 0),
			before: {
				year: initial.summary?.year,
				quarter: initial.summary?.quarter,
				date: initial.summary?.gameDate,
				stockIndex: initial.summary?.stockIndex,
			},
			after: {
				year: final.summary?.year,
				quarter: final.summary?.quarter,
				date: final.summary?.gameDate,
				stockIndex: final.summary?.stockIndex,
			},
		},
		null,
		2,
	),
);
