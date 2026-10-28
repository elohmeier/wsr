import { acknowledgeStartupQuote } from './browser-helpers.mjs';
// node web/reliability-browser-test.mjs wsr985.exe GAME01.DAT fixture-directory http://localhost:8899/
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
const [installer, save, fixtures, url = 'http://localhost:8899/'] =
	process.argv.slice(2);
if (!installer || !save || !fixtures)
	throw Error(
		'Provide installer, complete save and browser_fixtures output directory.',
	);
const temp = mkdtempSync(resolve(tmpdir(), 'wsr-reliability-'));
const initScript = resolve(temp, 'storage-faults.js');
writeFileSync(
	initScript,
	"window.storageFault='blocked';window.originalOpen=IDBFactory.prototype.open;IDBFactory.prototype.open=function(...args){if(storageFault==='blocked')throw new DOMException('Storage blocked','SecurityError');return originalOpen.apply(this,args)};window.originalTransaction=IDBDatabase.prototype.transaction;IDBDatabase.prototype.transaction=function(...args){const tx=originalTransaction.apply(this,args);if(args[1]==='readwrite'&&storageFault==='quota')queueMicrotask(()=>tx.abort());return tx};window.NativeWorker=Worker;window.Worker=class extends NativeWorker{constructor(...args){super(...args);window.testWorker=this}}",
);
const session = 'wsr-reliability-' + process.pid,
	bin = process.env.AGENT_BROWSER_BIN || 'agent-browser';
function browser(...args) {
	const r = JSON.parse(
		execFileSync(bin, ['--session', session, '--json', ...args], {
			encoding: 'utf8',
			timeout: 40000,
			maxBuffer: 4 * 1024 * 1024,
		}),
	);
	assert.ok(r.success, JSON.stringify(r.error));
	return r.data;
}
const evaluate = (js) => browser('eval', js).result,
	wait = (js) => browser('wait', '--fn', js),
	click = (selector) => browser('click', selector);
const button = (name) =>
	browser('find', 'role', 'button', 'click', '--name', name, '--exact');
const idle = () =>
	wait(
		"!document.querySelector('#dlg') && !document.querySelector('#installer').disabled",
	);
const files = () => {
	click('#menus [data-slot=menubar-trigger]:first-child');
	click('[data-files-menu]');
};
function dismissDividendRecovery() {
	wait(
		"!!document.querySelector('#dlg') || !document.querySelector('#installer').disabled",
	);
	if (
		evaluate(
			"document.querySelector('#dlg')?.textContent.includes('invalid dividends')",
		)
	) {
		assert.match(
			evaluate("document.querySelector('#dlg').textContent"),
			/reset to zero/,
		);
		button('OK');
	}
}
function dismissSave() {
	for (let i = 0; i < 6; i++) {
		if (!evaluate("!!document.querySelector('#dlg')")) break;
		const labels = evaluate(
			"[...document.querySelectorAll('#dlg-buttons button')].map(b=>b.textContent)",
		);
		button(labels.includes('Yes') ? 'Yes' : 'OK');
	}
	idle();
}
try {
	browser('open', '--init-script', initScript, url);
	wait("!document.querySelector('#installer').disabled");
	assert.match(
		evaluate("document.querySelector('#storage-warning').textContent"),
		/not been saved/,
	);
	browser('upload', '#installer', resolve(installer));
	await acknowledgeStartupQuote(browser);
	idle();
	browser('upload', '#save-file', resolve(save));
	dismissDividendRecovery();
	idle();
	click('#save-game');
	dismissSave();
	assert.match(
		evaluate("document.querySelector('#storage-warning').textContent"),
		/waiting to be stored/,
	);
	assert.doesNotMatch(
		evaluate("document.querySelector('#status').textContent"),
		/saved files kept/,
	);
	files();
	const name = evaluate("document.querySelector('#saved-games').value");
	const downloadPath = '/tmp/wsr-recovery-' + process.pid + '.DAT';
	browser('download', '#download', downloadPath);
	assert.ok(
		readFileSync(downloadPath).subarray(0, 7).toString().startsWith('@EC9.85'),
	);
	evaluate("storageFault='quota'");
	click('#retry-storage');
	idle();
	assert.match(
		evaluate("document.querySelector('#storage-warning').textContent"),
		/waiting to be stored/,
	);
	browser('fill', '#symbol', 'AAPL');
	browser('press', 'Enter');
	idle();
	assert.match(
		evaluate("document.querySelector('#selected-name').textContent"),
		/APPLE/i,
	);
	assert.ok(
		evaluate("!!document.querySelector('#storage-warning')"),
		'Another action hid the failed-storage warning',
	);
	evaluate("storageFault=''");
	click('#retry-storage');
	idle();
	wait("!document.querySelector('#storage-warning')");
	const saved = evaluate(
		`(async()=>{const db=await new Promise((resolve,reject)=>{const r=originalOpen.call(indexedDB,'wsr-local-game-files',1);r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error)});const file=await new Promise(resolve=>{const r=db.transaction('files').objectStore('files').get(${JSON.stringify(name)});r.onsuccess=()=>resolve(r.result)});db.close();return [...file.bytes.slice(0,7)]})()`,
	);
	assert.deepEqual(saved, [...readFileSync(downloadPath).subarray(0, 7)]);
	const selectedBeforeBadImport = evaluate(
		"document.querySelector('#selected-name').textContent",
	);
	browser('upload', '#save-file', resolve(fixtures, 'GAME47.DAT'));
	wait(
		"document.querySelector('#error')?.textContent.includes('ExchangeRate')",
	);
	idle();
	assert.match(
		evaluate("document.querySelector('#error').textContent"),
		/ExchangeRate.*not finite/,
	);
	assert.equal(
		evaluate("document.querySelector('#selected-name').textContent"),
		selectedBeforeBadImport,
	);
	assert.equal(evaluate("document.querySelector('#saved-games').value"), name);
	const afterBadImport = resolve(temp, 'after-bad-import.DAT');
	browser('download', '#download', afterBadImport);
	assert.deepEqual(readFileSync(afterBadImport), readFileSync(downloadPath));
	assert.equal(
		evaluate(
			"[...document.querySelector('#saved-games').options].some(o=>o.value==='GAME47.DAT')",
		),
		false,
	);
	browser('upload', '#save-file', resolve(fixtures, 'GAME46.DAT'));
	wait(
		"document.querySelector('#error')?.textContent.includes('positive conversion value')",
	);
	idle();
	assert.match(
		evaluate("document.querySelector('#error').textContent"),
		/BondIssueInfo\(\d+\).*positive conversion value/,
	);
	assert.equal(
		evaluate("document.querySelector('#selected-name').textContent"),
		selectedBeforeBadImport,
	);
	assert.equal(
		evaluate(
			"[...document.querySelector('#saved-games').options].some(o=>o.value==='GAME46.DAT')",
		),
		false,
	);
	browser('download', '#download', afterBadImport);
	assert.deepEqual(readFileSync(afterBadImport), readFileSync(downloadPath));
	// The live owner holds the browser's exclusive lock; another tab must not boot a writer.
	browser('tab');
	browser('tab', 'new', url);
	wait("!!document.querySelector('#storage-warning')");
	assert.match(
		evaluate("document.querySelector('#storage-warning').textContent"),
		/already open in another tab/,
	);
	assert.equal(evaluate("document.querySelector('#installer').disabled"), true);
	browser('tab', 'close');
	// Reload resets the fault injection; allow storage again once this new document is initialized.
	browser('reload');
	wait("!document.querySelector('#installer').disabled");
	evaluate("storageFault=''");
	click('#retry-storage');
	await acknowledgeStartupQuote(browser);
	idle();
	browser('upload', '#save-file', resolve(fixtures, 'GAME49.DAT'));
	dismissDividendRecovery();
	idle();
	assert.match(
		evaluate("document.querySelector('.wsr-status-strip').textContent"),
		/January 31/,
	);
	// SoloHumanMode is derived by the first turn, rather than serialized in GAMExx.DAT.
	click('[data-control="244"]');
	idle();
	assert.equal(
		evaluate('document.querySelector(\'[data-control="244"]\').disabled'),
		true,
		'End turn must be disabled for a solo human',
	);
	files();
	browser('upload', '#save-file', resolve(fixtures, 'GAME48.DAT'));
	dismissDividendRecovery();
	// The original modal pause deadline can defer the load lifecycle's first tick.
	wait(
		"!!document.querySelector('[data-report-chart]') || (!document.querySelector('#dlg') && !document.querySelector('#installer').disabled)",
	);
	if (!evaluate("!!document.querySelector('[data-report-chart]')"))
		click('#ticker-toggle');
	wait("!!document.querySelector('[data-report-chart]')");
	const title = evaluate("document.querySelector('#dlg-title').textContent");
	click('[data-report-chart]');
	wait("!!document.querySelector('#dlg .chart')");
	button('Close');
	wait("!!document.querySelector('[data-report-chart]')");
	assert.equal(
		evaluate("document.querySelector('#dlg-title').textContent"),
		title,
	);
	button('OK');
	idle();
	if (
		evaluate(
			"document.querySelector('.wsr-clock')?.dataset.running === 'true'",
		)
	) {
		click('#ticker-toggle');
		idle();
	}
	// Native MB_DEFBUTTON2 must focus No. Enter cancels the exit and leaves the game usable.
	click('#menus [data-slot=menubar-trigger]:first-child');
	browser('find', 'role', 'menuitem', 'click', '--name', 'Exit');
	wait("document.querySelector('#dlg-title') !== null");
	assert.equal(evaluate('document.activeElement.textContent'), 'No');
	browser('press', 'Escape');
	assert.ok(
		evaluate("!!document.querySelector('#dlg')"),
		'Yes/No message boxes have no implicit Cancel action',
	);
	browser('press', 'Enter');
	idle();
	// Research metadata reaches the real host packet, including non-actionable Hold ratings.
	browser('fill', '#symbol', 'AAPL');
	browser('press', 'Enter');
	idle();
	click('[data-control="130"]');
	wait("!!document.querySelector('[data-report-rating]')");
	assert.match(
		evaluate("document.querySelector('[data-report-rating]').textContent"),
		/buy|sell|hold|not traded/i,
	);
	button('OK');
	idle();
	evaluate(
		"testWorker.dispatchEvent(new ErrorEvent('error',{message:'Regression: worker stopped'}))",
	);
	wait(
		"document.querySelector('#files-panel').open && !document.querySelector('#download').disabled",
	);
	assert.equal(evaluate("document.querySelector('#symbol').readOnly"), true);
	const crashDownload = resolve(temp, 'after-worker-crash.DAT');
	browser('download', '#download', crashDownload);
	assert.ok(
		readFileSync(crashDownload).subarray(0, 7).toString().startsWith('@EC9.85'),
	);
	console.log(
		'Browser reliability: unavailable storage, quota abort, durable retry, rejected corrupt import with live-game/save preservation, exclusive tabs, solo controls, month-end date, nested earnings chart, analyst rating, message-box defaults/cancel and recovery download after a worker failure passed.',
	);
} catch (error) {
	try {
		console.error(
			evaluate(
				"({dialog:document.querySelector('#dlg')?.textContent,error:document.querySelector('#error')?.textContent,storage:document.querySelector('#storage-warning')?.textContent})",
			),
		);
	} catch {}
	throw error;
} finally {
	browser('close');
	rmSync(temp, { recursive: true, force: true });
}
