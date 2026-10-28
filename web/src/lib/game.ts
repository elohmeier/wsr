import { writable, get } from 'svelte/store';
import { toast } from 'svelte-sonner';
import { cacheAssets, asset } from '../../docs.mjs';
import type { Json, Snapshot } from './coach/types';
import type {
	GameFile,
	HostRequest,
	MenuItem,
	Panel,
	Result,
	StoredFile,
	Summary,
	UIRequest,
} from './types';
export interface ViewState {
	ready: boolean;
	busy: boolean;
	status: string;
	progress: string;
	error: string;
	storageError: string;
	storageBlocked: boolean;
	canRetryStorage: boolean;
	unsavedFiles: number;
	summary: Summary | null;
	files: GameFile[];
	menus: MenuItem[];
	countries: string[];
	requests: HostRequest[];
	ui: UIRequest | null;
	panel: Panel;
	coachOpen: boolean;
	coachOpened: boolean;
	coachSnapshot: Snapshot | null;
	coachSession: string;
	gameRevision: number;
}
const initial: ViewState = {
	ready: false,
	busy: false,
	status: 'Starting engine…',
	progress: '',
	error: '',
	storageError: '',
	storageBlocked: false,
	canRetryStorage: true,
	unsavedFiles: 0,
	summary: null,
	files: [],
	menus: [],
	countries: [],
	requests: [],
	ui: null,
	panel: null,
	coachOpen: false,
	coachOpened: false,
	coachSnapshot: null,
	coachSession: '',
	gameRevision: 0,
};
const store = writable<ViewState>(initial);
let worker: Worker,
	ctrl: Int32Array,
	answerData: Uint8Array,
	timer: ReturnType<typeof setTimeout>,
	nextId = 1,
	token = 1;
let inFlight: Promise<void> = Promise.resolve();
const pending = new Map<
	number,
	{ resolve: (value: any) => void; reject: (error: Error) => void }
>();
let db: IDBDatabase | null = null;
const fileCache = new Map<string, Uint8Array>();
let storageQueue: Promise<void> = Promise.resolve();
let ownsStorage = false;

async function acquireStorage(): Promise<boolean> {
	if (!navigator.locks) {
		patch({
			canRetryStorage: false,
			storageError:
				'This browser cannot protect saves from other tabs. Download saves to keep them.',
		});
		return true; // Play in memory; persist() refuses writes without ownership.
	}
	return new Promise((resolve, reject) => {
		void navigator.locks
			.request(
				'wsr-local-game-files-writer',
				{ ifAvailable: true },
				async (lock) => {
					ownsStorage = !!lock;
					resolve(ownsStorage);
					if (lock) await new Promise<void>(() => {}); // Released automatically when this tab closes.
				},
			)
			.catch(reject);
	});
}

function remember(files: StoredFile[]) {
	for (const f of files)
		f.bytes === null
			? fileCache.delete(f.name)
			: fileCache.set(f.name, f.bytes);
	cacheAssets(files);
}

function storageFailure(error: unknown) {
	patch({
		storageError:
			'Files have not been saved in this browser. Retry storage or download your saved game. ' +
			String(error),
	});
}
function patch(values: Partial<ViewState>) {
	store.update((s) => ({ ...s, ...values }));
}
async function openStorage(): Promise<StoredFile[]> {
	try {
		db = await new Promise<IDBDatabase>((resolve, reject) => {
			const request = indexedDB.open('wsr-local-game-files', 1);
			request.onupgradeneeded = () =>
				request.result.createObjectStore('files', { keyPath: 'name' });
			request.onsuccess = () => resolve(request.result);
			request.onerror = () => reject(request.error);
		});
		const files = await new Promise<StoredFile[]>((resolve, reject) => {
			const r = db!.transaction('files').objectStore('files').getAll();
			r.onsuccess = () => resolve(r.result);
			r.onerror = () => reject(r.error);
		});
		db.onversionchange = () => {
			db?.close();
			db = null;
			storageFailure('Browser storage was closed by another page.');
		};
		if (ownsStorage && !get(store).unsavedFiles) patch({ storageError: '' });
		return files;
	} catch (error) {
		db?.close();
		db = null;
		storageFailure(error);
		return [];
	}
}
async function persist(files: StoredFile[] = []) {
	if (!files.length) return;
	if (!ownsStorage || !db) throw new Error('Browser storage is unavailable.');
	await new Promise<void>((resolve, reject) => {
		const tx = db!.transaction('files', 'readwrite'),
			target = tx.objectStore('files');
		for (const f of files)
			f.bytes === null ? target.delete(f.name) : target.put(f);
		tx.oncomplete = () => resolve();
		tx.onerror = () => reject(tx.error);
		tx.onabort = () => reject(tx.error || new Error('Save interrupted'));
	});
}
const rpc = (message: Record<string, unknown>) =>
	new Promise<any>((resolve, reject) => {
		if (!get(store).ready) {
			reject(
				new Error(
					'The engine is stopped. Download your saved game and reload to continue.',
				),
			);
			return;
		}
		const id = nextId++;
		pending.set(id, { resolve, reject });
		worker.postMessage({ ...message, id });
	});
function accept(result: Result) {
	if (!result.ok)
		throw new Error(result.error || 'The game could not complete that action.');
	store.update((s) => ({
		...s,
		summary: result.summary === undefined ? s.summary : result.summary,
		files: result.files || s.files,
		menus: result.menus || s.menus,
		countries: result.countries || s.countries,
		requests: result.requests || [],
	}));
	for (const request of result.requests || []) {
		if (request.kind === 'program' && /WSRHELP/i.test(request.value || ''))
			openDocs();
		if (request.kind === 'url' && /WSRBOOK\.HTM/i.test(request.value || ''))
			openDocs('manual');
	}
}
function schedule() {
	clearTimeout(timer);
	const state = get(store),
		s = state.summary;
	if (
		state.ready &&
		!state.busy &&
		!state.panel &&
		!state.coachOpen &&
		s?.clock === 'RUN' &&
		!s.gameOver
	)
		timer = setTimeout(
			() => run({ cmd: 'action', action: 5 }, true),
			Math.max(20, (100 - s.tickerSpeed) * 2),
		);
}
async function run(message: Record<string, unknown>, background = false) {
	const current = get(store);
	if (!current.ready || current.busy) return;
	clearTimeout(timer);
	if (!background) patch({ busy: true, progress: 'Working…' });
	const previous = inFlight;
	inFlight = (async () => {
		try {
			await previous;
			accept(await rpc(message));
			if (get(store).ready) {
				if (!background) patch({ error: '' });
				patch({ status: 'Ready' });
			}
		} catch (error) {
			patch({ error: error instanceof Error ? error.message : String(error) });
		}
	})();
	await inFlight;
	if (!background) patch({ busy: false, progress: '' });
	schedule();
}
export function answer(value: number, bytes = new Uint8Array()) {
	if (!get(store).ui) return;
	patch({ ui: null });
	const size = Math.min(bytes.length, answerData.length);
	answerData.set(bytes.subarray(0, size));
	ctrl[2] = size;
	ctrl[1] = value;
	Atomics.store(ctrl, 0, 1);
	Atomics.notify(ctrl, 0);
}
export function openDocs(
	mode: 'help' | 'glossary' | 'manual' = 'help',
	query = '',
) {
	clearTimeout(timer);
	patch({ panel: { kind: 'docs', mode, query } });
}
export function closePanel() {
	patch({ panel: null });
	schedule();
}
export function openCoach() {
	clearTimeout(timer);
	patch({ coachOpen: true, coachOpened: true });
}
export function closeCoach() {
	patch({ coachOpen: false });
	schedule();
}
/** Race a queued read against a new blocking dialog; waiting for inFlight here can deadlock. */
export async function captureCoach(): Promise<Snapshot | null> {
	const withScreen = (snapshot: Snapshot | null): Snapshot | null => {
		const state = get(store);
		const baseline = snapshot ?? {
			sessionId: state.coachSession,
			revision: state.gameRevision,
			capturedAt: new Date().toISOString(),
			observation: {
				schemaVersion: 1,
				available: false,
				reason: 'No game snapshot is available yet.',
			},
		};
		return {
			...baseline,
			pendingDialog: !!state.ui,
			screen: state.ui
				? (JSON.parse(JSON.stringify(state.ui)) as Json)
				: undefined,
		};
	};
	if (get(store).ui || !get(store).ready)
		return withScreen(get(store).coachSnapshot);
	return new Promise((resolve) => {
		let settled = false;
		let unsubscribe = () => {};
		const finish = (snapshot: Snapshot | null) => {
			if (settled) return;
			settled = true;
			unsubscribe();
			resolve(withScreen(snapshot));
		};
		unsubscribe = store.subscribe((state) => {
			if (state.ui || !state.ready) finish(state.coachSnapshot);
		});
		if (settled) {
			unsubscribe();
			return;
		}
		void rpc({ cmd: 'observe' }).then(finish, () =>
			finish(get(store).coachSnapshot),
		);
	});
}
export async function openList(kind: 'players' | 'companies') {
	if (get(store).busy) return;
	clearTimeout(timer);
	await inFlight;
	clearTimeout(timer);
	patch({ panel: { kind } });
}
export async function openCustomizer() {
	if (get(store).busy || !asset('CORPNAME.DAT')) return;
	clearTimeout(timer);
	patch({ busy: true });
	await inFlight;
	clearTimeout(timer);
	patch({ busy: false, panel: { kind: 'customizer' } });
}
export async function customize(bytes: Uint8Array) {
	patch({ busy: true });
	try {
		await inFlight;
		accept(await rpc({ cmd: 'customize', bytes }));
		return !get(store).storageError;
	} finally {
		patch({ busy: false });
		schedule();
	}
}
export async function start() {
	if (worker) return;
	if (!crossOriginIsolated) {
		patch({
			error:
				'This page needs cross-origin isolation. Use the supplied development or preview server.',
		});
		return;
	}
	try {
		if (!(await acquireStorage())) {
			patch({
				storageBlocked: true,
				status: 'Open in another tab',
				storageError:
					'Wall Street Raider is already open in another tab. Close that tab, then reload this page to use these saves.',
			});
			return;
		}
	} catch (error) {
		storageFailure(error);
		return;
	}
	const shared = new SharedArrayBuffer(16 + (1 << 20));
	ctrl = new Int32Array(shared, 0, 4);
	answerData = new Uint8Array(shared, 16);
	worker = new Worker(new URL('../../worker.js', import.meta.url), {
		type: 'module',
	});
	worker.onmessage = async ({ data: m }) => {
		if (m.type === 'observation')
			patch({
				coachSnapshot: m.snapshot,
				coachSession: m.snapshot.sessionId,
				gameRevision: m.snapshot.revision,
			});
		else if (m.type === 'coach-scope')
			patch({
				coachSnapshot: null,
				coachSession: m.sessionId,
			});
		else if (m.type === 'ui') patch({ ui: { ...m, token: token++ } });
		else if (m.type === 'notice')
			toast(m.text, { duration: 10000, closeButton: true });
		else if (m.type === 'progress') patch({ progress: m.text });
		else if (m.type === 'ready') {
			accept(m.value);
			patch({
				ready: true,
				status: 'Ready',
				coachSession: m.coachSession ?? '',
			});
			schedule();
		} else if (m.type === 'result') {
			if (m.gameRevision !== undefined) patch({ gameRevision: m.gameRevision });
			const p = pending.get(m.id);
			pending.delete(m.id);
			// Inspection has no file journal. It must not clear pending save warnings/counts.
			if (m.observation) {
				p?.resolve(m.value);
				return;
			}
			// Process results in delivery order. A storage failure cannot undo a completed
			// engine action: keep the view current and retain the worker's unacknowledged files.
			storageQueue = storageQueue.then(async () => {
				remember(m.changes || []);
				patch({ unsavedFiles: m.changes?.length || 0 });
				try {
					await persist(m.changes);
					if (m.changes?.length) {
						worker.postMessage({ cmd: 'ack', revision: m.revision });
						patch({ storageError: '', unsavedFiles: 0 });
					}
				} catch (error) {
					storageFailure(error);
				}
				p?.resolve(m.value);
			});
		} else if (m.type === 'error') {
			const p = pending.get(m.id);
			pending.delete(m.id);
			if (p) p.reject(new Error(m.error));
			else patch({ error: m.error });
		}
	};
	worker.onerror = (e) => {
		clearTimeout(timer);
		patch({
			error:
				e.message +
				' Download your saved game, then reload to restart the engine.',
			status: 'Engine stopped',
			progress: '',
			ready: false,
			busy: false,
			ui: null,
		});
		for (const p of pending.values()) p.reject(new Error(e.message));
		pending.clear();
	};
	const files = await openStorage();
	remember(files);
	patch({
		files: files
			.filter((f) => f.bytes !== null)
			.map((f) => ({ name: f.name, size: f.bytes!.length })),
	});
	worker.postMessage({ cmd: 'init', shared, files });
	window.addEventListener('beforeunload', (event) => {
		if (get(store).unsavedFiles) {
			event.preventDefault();
			event.returnValue = '';
		}
	});
}
export async function retryStorage() {
	if (get(store).busy || get(store).storageBlocked || !ownsStorage) return;
	patch({ busy: true, progress: 'Retrying browser storage…' });
	let restored: StoredFile[] | undefined;
	try {
		await inFlight;
		if (!db) {
			restored = await openStorage();
			remember(restored.filter((f) => !fileCache.has(f.name)));
		}
	} finally {
		patch({ busy: false, progress: '' });
	}
	if (db)
		await run(
			restored ? { cmd: 'restore', files: restored } : { cmd: 'flush' },
		);
}
export async function resume(name: string, fixed: boolean) {
	if (!get(store).ready || get(store).busy) return;
	const bytes = await rpc({ cmd: 'getfile', name });
	await run({ cmd: 'load', name, bytes, fixed });
}
export async function download(name: string) {
	const bytes = fileCache.get(name);
	if (!bytes) {
		patch({ error: 'No saved file is available to download.' });
		return;
	}
	const url = URL.createObjectURL(new Blob([bytes as BlobPart]));
	const a = document.createElement('a');
	a.href = url;
	a.download = name;
	a.click();
	setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export const game = {
	subscribe: store.subscribe,
	run,
	action: (action: number, arg = 0, text?: Uint8Array) =>
		run({ cmd: 'action', action, arg, text }),
	report: (kind: number, entity = 0) => run({ cmd: 'report', kind, entity }),
	error: (error: string) => patch({ error }),
};
