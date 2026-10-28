// node web/ui-parity-browser-test.mjs INSTALLER SAVE [URL] [GAME45 commodity fixture]
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { acknowledgeStartupQuote } from './browser-helpers.mjs';
const [installer, save, url = 'http://localhost:8899/', commoditySave] =
	process.argv.slice(2);
if (!installer || !save)
	throw new Error(
		'Provide the original installer and an unfinished human-player save.',
	);
const session = 'wsr-ui-parity-' + process.pid;
function browser(...args) {
	const result = JSON.parse(
		execFileSync(
			process.env.AGENT_BROWSER_BIN || 'agent-browser',
			['--session', session, '--json', ...args],
			{ encoding: 'utf8', timeout: 40000, maxBuffer: 4 * 1024 * 1024 },
		),
	);
	assert.ok(result.success, JSON.stringify(result.error));
	return result.data;
}
const evaluate = (js) => browser('eval', js).result;
const wait = (js) => browser('wait', '--fn', js);
const click = (selector) => browser('click', selector);
const button = (name) =>
	browser('find', 'role', 'button', 'click', '--name', name, '--exact');
const idle = () =>
	wait(
		"!document.querySelector('#dlg') && !document.querySelector('#installer').disabled",
	);
const search = () => wait("!!document.querySelector('[data-database-search]')");
const field = (id) => '#database-control-' + id;
function menu(group, match) {
	browser('find', 'role', 'menuitem', 'click', '--name', group, '--exact');
	const label = evaluate(
		`[...document.querySelectorAll('[role=menuitem]')].map(e=>e.textContent.trim()).find(t=>${match}.test(t))`,
	);
	assert.ok(label);
	browser('find', 'role', 'menuitem', 'click', '--name', label, '--exact');
}
function chartSize() {
	click('[data-control="114"]');
	wait("!!document.querySelector('.chart [data-chart]')");
	const size = evaluate(
		"({width:document.querySelector('#dlg').getBoundingClientRect().width,height:document.querySelector('.chart [data-chart]').getBoundingClientRect().height,last:document.querySelector('[data-chart-last]').textContent})",
	);
	assert.ok(size.last.trim());
	button('Close');
	idle();
	return size;
}
try {
	browser('open', url);
	browser('set', 'viewport', '1366', '768');
	wait("!document.querySelector('#installer').disabled");
	browser('upload', '#installer', resolve(installer));
	await acknowledgeStartupQuote(browser);
	idle();
	browser('upload', '#save-file', resolve(save));
	wait(
		"!!document.querySelector('#dlg') || !document.querySelector('#installer').disabled",
	);
	if (evaluate("!!document.querySelector('#dlg')")) button('OK');
	idle();
	if (
		evaluate("document.querySelector('.wsr-clock')?.dataset.running==='true'")
	) {
		click('#ticker-toggle');
		idle();
	}
	// The web Customizer is available without retaining the Windows executable.
	menu('Game Options', /Customizer/i);
	wait("!!document.querySelector('.customizer')");
	click('.customizer [data-close]');
	idle();
	click('[data-control="104"]');
	button('Database Search');
	search();
	assert.equal(
		evaluate(
			"document.querySelectorAll('[data-database-search] input[type=checkbox]').length",
		),
		19,
	);
	assert.equal(
		evaluate(
			"[...document.querySelectorAll('[data-database-search] input')].filter(i=>i.type==='text').length",
		),
		7,
	);
	browser('fill', field(288), '25');
	wait(
		"document.querySelector('#database-control-286')?.checked && document.querySelector('#database-control-288')?.value==='25'",
	);
	button('Change management rating');
	search();
	assert.equal(
		evaluate(
			'document.querySelector(\'[data-search-rating="375"]\').textContent',
		),
		'Average',
	);
	button('Help');
	wait("!!document.querySelector('.documentation')");
	assert.equal(
		evaluate("document.querySelector('.documentation [data-search]').value"),
		'database search',
	);
	click('.documentation [data-close]');
	search();
	assert.equal(
		evaluate("document.querySelector('#database-control-288').value"),
		'25',
	);
	// F1 uses the same topic and preserves unsubmitted input.
	browser('fill', field(288), '26');
	browser('press', 'F1');
	wait("!!document.querySelector('.documentation')");
	click('.documentation [data-close]');
	search();
	wait(
		"document.querySelector('#database-control-288')?.value==='26' && document.querySelector('#database-control-286')?.checked",
	);
	button('Clear Checkboxes');
	search();
	assert.equal(
		evaluate(
			"document.querySelectorAll('[data-database-search] input:checked').length",
		),
		0,
	);
	browser('set', 'viewport', '390', '844');
	assert.equal(evaluate('document.documentElement.scrollWidth'), 390);
	assert.ok(
		evaluate(
			"[...document.querySelectorAll('#dlg-buttons button')].every(b=>b.getBoundingClientRect().bottom<=innerHeight)",
		),
	);
	browser('set', 'viewport', '1366', '768');
	button('Close');
	button('Who Owns What?');
	wait("!!document.querySelector('[data-selector]')");
	assert.equal(
		evaluate(
			"document.querySelectorAll('[data-selector] input[type=radio]').length",
		),
		6,
	);
	evaluate("window.selector=document.querySelector('[data-selector]')");
	browser('check', '[data-selector] input[value="1"]');
	assert.equal(
		evaluate("document.querySelector('[data-selector]')===selector"),
		true,
		'Selection must wait for Go',
	);
	button('Cancel');
	button('CLOSE');
	idle();
	click('[data-control="110"]');
	button('Interest Rate Swaps');
	browser('fill', '#game-field-989', '1,234');
	button('SWAP INFO');
	wait("!!document.querySelector('.documentation')");
	assert.equal(
		evaluate("document.querySelector('.documentation [data-search]').value"),
		'interest rate swaps',
	);
	click('.documentation [data-close]');
	assert.equal(
		evaluate("document.querySelector('#game-field-989').value"),
		'1,234',
	);
	button('EXIT');
	button('CLOSE');
	idle();
	const first = chartSize();
	menu('Settings', /Chart.*Size/i);
	wait(
		"document.querySelector('#dlg')?.dataset.kind==='chart' || !document.querySelector('#installer').disabled",
	);
	if (evaluate("document.querySelector('#dlg')?.dataset.kind==='chart'")) {
		button('Close');
		idle();
	}
	const second = chartSize();
	assert.ok(Math.abs(first.width - second.width) > 200);
	assert.ok(Math.abs(first.height - second.height) > 100);
	browser('fill', '#symbol', 'AAPL');
	browser('press', 'Enter');
	idle();
	click('[data-control="107"]');
	const advanced = evaluate(
		"[...document.querySelectorAll('#dlg button')].map(b=>b.textContent.trim()).find(t=>/Advanced Option/i.test(t))",
	);
	button(advanced);
	wait("!!document.querySelector('.option-grid')");
	assert.match(
		evaluate("document.querySelector('.option-grid thead').textContent"),
		/1–10%/,
	);
	assert.match(
		evaluate("document.querySelector('#dlg-buttons').textContent"),
		/close all.*AAPL.*options/i,
	);
	button('Close');
	button('CLOSE');
	idle();
	if (commoditySave) {
		browser('upload', '#save-file', resolve(commoditySave));
		idle();
		click('[data-control="107"]');
		button('Trade Commodities');
		button('CRUDE OIL');
		const mode = evaluate(
			"[...document.querySelectorAll('#dlg [data-choice]')].map(b=>b.textContent.trim()).find(t=>t.includes('PHYSICAL'))",
		);
		if (mode) button(mode);
		wait(
			"[...document.querySelectorAll('#dlg [data-choice]')].filter(b=>b.disabled).length===2",
		);
		assert.deepEqual(
			evaluate(
				"[...document.querySelectorAll('#dlg [data-choice]:disabled')].map(b=>b.textContent.trim())",
			),
			['COVER SHORT', 'SELL SHORT'],
		);
		button('CLOSE');
		button('CLOSE');
		idle();
		console.log(
			'Commodity desk: physical trading retains visible disabled short/cover controls.',
		);
	}
	console.log(
		'UI parity: Customizer menu, database controls/edits/ratings/count/help/F1/clear/mobile, explicit radio selection, swap help, chart sizes/latest values and option size/scope labels passed.',
	);
} catch (error) {
	console.error(
		evaluate(
			"({dialog:document.querySelector('#dlg')?.textContent,error:document.querySelector('#error:not([hidden])')?.textContent})",
		),
	);
	throw error;
} finally {
	browser('close');
}
