// Storage failure regressions exercise the real client with a worker/IndexedDB boundary fake.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import { FileJournal } from './file-journal.mjs';
const journal = new FileJournal();
journal.record([{ name: 'GAME01.DAT', bytes: new Uint8Array([1]) }]);
const old = journal.snapshot();
journal.record([
	{ name: 'GAME01.DAT', bytes: new Uint8Array([2]) },
	{ name: 'OLD.DAT', bytes: null },
]);
journal.acknowledge(old.revision);
assert.equal(
	journal.snapshot().changes[0].bytes[0],
	2,
	'A late acknowledgement discarded a newer save',
);
journal.acknowledge(journal.snapshot().revision);
assert.deepEqual(journal.snapshot().changes, []);
const source = readFileSync(
	new URL('./src/lib/game.ts', import.meta.url),
	'utf8',
).replace("new URL('../../worker.js', import.meta.url)", "'worker.js'");
const code = ts.transpileModule(source, {
	compilerOptions: {
		module: ts.ModuleKind.CommonJS,
		target: ts.ScriptTarget.ES2022,
	},
}).outputText;
const pause = () => new Promise((resolve) => setImmediate(resolve));
async function client(mode) {
	const timers = new Set();
	let timerId = 0;
	const clock = mode === 'coach-ticker' ? 'RUN' : 'STOP';
	const latest = new FileJournal(),
		disk = new Map(),
		downloads = [],
		acknowledgements = [];
	let state,
		worker,
		fail = mode,
		writes = 0,
		releases = 0;
	const writable = (initial) => {
		let value = initial;
		const subscribers = [];
		return {
			subscribe(fn) {
				subscribers.push(fn);
				fn(value);
				return () => {};
			},
			update(fn) {
				value = fn(value);
				subscribers.forEach((f) => f(value));
			},
		};
	};
	const get = (store) => {
		let value;
		store.subscribe((v) => (value = v))();
		return value;
	};
	const indexedDB = {
		open() {
			if (fail === 'unavailable') throw Error('Storage blocked');
			const request = {};
			queueMicrotask(() => {
				request.result = {
					close() {},
					transaction(_name, kind) {
						if (kind !== 'readwrite')
							return {
								objectStore: () => ({
									getAll() {
										const r = {
											result: [...disk].map(([name, bytes]) => ({
												name,
												bytes,
											})),
										};
										queueMicrotask(() => r.onsuccess());
										return r;
									},
								}),
							};
						const staged = [];
						const tx = {
							error: Error('Quota exhausted'),
							objectStore: () => ({
								put(file) {
									writes++;
									staged.push(file);
								},
								delete(name) {
									staged.push({ name, bytes: null });
								},
							}),
						};
						queueMicrotask(() => {
							if (mode === 'crash-during-save')
								worker.onerror({ message: 'Engine crashed during save' });
							if (fail === 'quota') tx.onabort();
							else {
								for (const f of staged)
									f.bytes === null
										? disk.delete(f.name)
										: disk.set(f.name, f.bytes);
								tx.oncomplete();
							}
						});
						return tx;
					},
				};
				request.onsuccess();
			});
			return request;
		},
	};
	class Worker {
		constructor() {
			worker = this;
		}
		postMessage(m) {
			if (m.cmd === 'ack') {
				acknowledgements.push(m.revision);
				latest.acknowledge(m.revision);
				return;
			}
			queueMicrotask(() => {
				if (m.cmd === 'observe') {
					if (mode === 'coach-race') {
						this.onmessage({
							data: {
								type: 'ui',
								kind: 'inputbox',
								title: 'Order',
								prompt: 'Amount',
								value: '123',
							},
						});
					} else
						this.onmessage({
							data: {
								type: 'result',
								id: m.id,
								observation: true,
								value: {
									sessionId: 'test',
									revision: 1,
									capturedAt: 'now',
									observation: { available: true },
								},
							},
						});
					return;
				}
				if (m.cmd === 'init') {
					this.onmessage({
						data: {
							type: 'ready',
							value: { ok: true, summary: { clock, version: 0 } },
						},
					});
					return;
				}
				if (m.arg === 1)
					latest.record([{ name: 'GAME01.DAT', bytes: new Uint8Array([42]) }]);
				this.onmessage({
					data: {
						type: 'result',
						id: m.id,
						value: {
							ok: true,
							summary: { clock, version: m.arg || 3 },
						},
						...latest.snapshot(),
					},
				});
			});
		}
	}
	const exports = {};
	vm.runInNewContext(code, {
		exports,
		require(name) {
			if (name === 'svelte/store') return { writable, get };
			if (name === 'svelte-sonner') return { toast() {} };
			if (name.includes('docs.mjs')) return { cacheAssets() {}, asset() {} };
			throw Error(name);
		},
		Worker,
		indexedDB,
		navigator: {
			locks: {
				request: async (_name, _opts, callback) =>
					callback(mode === 'other-tab' ? null : {}),
			},
		},
		window: { addEventListener() {} },
		SharedArrayBuffer,
		Int32Array,
		Uint8Array,
		Atomics,
		crossOriginIsolated: true,
		setTimeout() {
			timers.add(++timerId);
			return timerId;
		},
		clearTimeout(id) {
			timers.delete(id);
		},
		URL: {
			createObjectURL(blob) {
				downloads.push(blob);
				return 'blob:save';
			},
			revokeObjectURL() {},
		},
		Blob,
		document: { createElement: () => ({ click() {} }) },
	});
	exports.game.subscribe((s) => (state = s));
	await exports.start();
	await pause();
	if (mode === 'coach-ticker') {
		assert.equal(timers.size, 1);
		exports.openCoach();
		assert.equal(timers.size, 0, 'The coach must suspend background ticks');
		exports.openDocs();
		exports.closeCoach();
		assert.equal(
			timers.size,
			0,
			'Closing the coach must respect another open panel',
		);
		exports.closePanel();
		assert.equal(
			timers.size,
			1,
			'The running ticker resumes when panels close',
		);
		return;
	}
	if (mode === 'coach-race') {
		const baseline = {
			sessionId: 'test',
			revision: 1,
			capturedAt: 'before dialog',
			observation: { available: true },
		};
		worker.onmessage({ data: { type: 'observation', snapshot: baseline } });
		exports.openCoach();
		const captured = await exports.captureCoach();
		assert.equal(
			captured.pendingDialog,
			true,
			'A dialog appearing during capture must not deadlock',
		);
		assert.equal(captured.capturedAt, baseline.capturedAt);
		assert.equal(captured.screen.value, '123');
		exports.closeCoach();
		assert.equal(
			state.ui.value,
			'123',
			'Closing the coach must not answer the engine dialog',
		);
		assert.equal(state.coachOpen, false);
		return;
	}
	if (mode === 'other-tab') {
		assert.equal(state.ready, false);
		assert.equal(state.storageBlocked, true);
		assert.equal(worker, undefined);
		return;
	}
	await exports.game.action(3, 1);
	await pause();
	if (mode === 'crash-during-save') {
		assert.equal(state.ready, false);
		assert.equal(state.status, 'Engine stopped');
		assert.match(state.error, /Engine crashed during save/);
		await exports.download('GAME01.DAT');
		assert.equal(new Uint8Array(await downloads[0].arrayBuffer())[0], 42);
		return;
	}
	assert.equal(
		state.summary.version,
		1,
		'Storage failure left the game view stale',
	);
	assert.ok(state.storageError, 'Storage failure was hidden');
	assert.equal(state.unsavedFiles, 1);
	assert.equal(acknowledgements.length, 0);
	assert.equal(disk.size, 0);
	await exports.captureCoach();
	assert.equal(
		state.unsavedFiles,
		1,
		'Read-only inspection must preserve pending saved files',
	);
	assert.ok(state.storageError);
	await exports.game.action(2, 2);
	await pause();
	assert.equal(state.summary.version, 2);
	assert.ok(
		state.storageError,
		'An unrelated action erased the storage warning',
	);
	assert.equal(acknowledgements.length, 0);
	await exports.download('GAME01.DAT');
	assert.equal(new Uint8Array(await downloads[0].arrayBuffer())[0], 42);
	fail = '';
	await exports.retryStorage();
	await pause();
	assert.equal(state.storageError, '');
	assert.equal(state.unsavedFiles, 0);
	assert.equal(disk.get('GAME01.DAT')[0], 42);
	assert.equal(acknowledgements.length, 1);
	assert.equal(latest.snapshot().changes.length, 0);
	worker.onerror({ message: 'Engine crashed' });
	assert.equal(state.ready, false);
	await exports.download('GAME01.DAT');
	assert.equal(
		downloads.length,
		2,
		'Worker failure prevented saved-file recovery',
	);
}
await client('unavailable');
await client('quota');
await client('other-tab');
await client('crash-during-save');
await client('coach-race');
await client('coach-ticker');
console.log(
	'Client: storage failures, retained saves, retry/ack, recovery, exclusive ownership, coach/dialog capture race and ticker suspension passed.',
);
