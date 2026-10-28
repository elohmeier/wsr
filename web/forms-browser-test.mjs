// Real browser + worker + WASM regressions; requires the owner's installer.
// node web/forms-browser-test.mjs wsr985.exe http://localhost:8899/
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { acknowledgeStartupQuote } from './browser-helpers.mjs';
const [installer, url = 'http://localhost:8899/'] = process.argv.slice(2);
if (!installer) throw new Error('Provide the original installer.');
const session = 'wsr-forms-' + process.pid;
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
const fill = (selector, value) => browser('fill', selector, String(value));
const button = (name) =>
	browser('find', 'role', 'button', 'click', '--name', name, '--exact');
const idle = () =>
	wait(
		"!document.querySelector('#dlg') && !document.querySelector('#installer').disabled",
	);
const values = () =>
	evaluate(
		"[...document.querySelectorAll('[data-game-form] input')].map(i=>i.value)",
	);
function setup(computer, players) {
	click('#new-game');
	button(computer ? 'Yes' : 'No');
	fill('#dlg-input', players);
	button('OK');
	if (computer && players > 2) {
		fill('#dlg-input', 1);
		button('OK');
	}
	wait("!!document.querySelector('[data-game-form]')");
}
try {
	browser('open', url);
	browser('set', 'viewport', '1366', '768');
	wait("!document.querySelector('#installer').disabled");
	browser('upload', '#installer', resolve(installer));
	await acknowledgeStartupQuote(browser);
	idle();
	setup(true, 5);
	assert.equal(values().length, 8);
	assert.equal(
		evaluate(
			"[...document.querySelectorAll('[data-game-form] label')].filter(l=>l.textContent.includes('(Computer)')).length",
		),
		4,
	);
	assert.equal(
		evaluate("document.querySelector('#game-field-342').maxLength"),
		20,
	);
	browser('set', 'viewport', '390', '844');
	assert.equal(evaluate('document.documentElement.scrollWidth'), 390);
	assert.ok(
		evaluate(
			"[...document.querySelectorAll('#dlg-buttons button')].every(b=>b.getBoundingClientRect().bottom<=innerHeight)",
		),
	);
	button('Cancel');
	idle();
	assert.equal(
		evaluate("!!document.querySelector('#desk')"),
		false,
		'Cancelling setup must not build a game',
	);
	browser('set', 'viewport', '1366', '768');
	setup(false, 2);
	fill('#game-field-342', 'Browser Alice');
	fill('#game-field-344', 'Browser Bobby');
	fill('#game-field-352', 800);
	fill('#game-field-354', 50);
	// Empty/invalid values keep the same form and all other entries editable.
	fill('#game-field-344', '');
	button('OK');
	assert.deepEqual(values(), ['Browser Alice', '', '800', '50', '2']);
	fill('#game-field-344', 'Browser Bobby');
	fill('#game-field-352', 99);
	button('OK');
	assert.deepEqual(values(), [
		'Browser Alice',
		'Browser Bobby',
		'99',
		'50',
		'2',
	]);
	fill('#game-field-352', 800);
	button('OK');
	wait(
		"document.querySelector('#dlg-title').textContent.includes('Disqualified')",
	);
	button('No');
	wait("!!document.querySelector('[data-game-form]')");
	assert.deepEqual(values(), [
		'Browser Alice',
		'Browser Bobby',
		'800',
		'35',
		'2',
	]);
	fill('#game-field-354', 10);
	button('OK');
	wait(
		"document.querySelector('#dlg-title').textContent.includes('Ready to start')",
	);
	button('OK');
	for (let i = 0; i < 8; i++) {
		wait(
			"!!document.querySelector('#dlg') || !document.querySelector('#installer').disabled",
		);
		if (!evaluate("!!document.querySelector('#dlg')")) break;
		button('OK');
	}
	idle();
	if (
		evaluate("document.querySelector('.wsr-clock')?.dataset.running==='true'")
	) {
		click('#ticker-toggle');
		idle();
	}
	click('[data-control="105"]');
	wait("document.querySelector('#dlg')?.dataset.kind==='symbolInput'");
	fill('#dlg-input', 'MSFT');
	button('Look up');
	assert.equal(evaluate('document.activeElement.id'), 'lookup-search');
	fill('#lookup-search', 'apple');
	assert.ok(evaluate("!!document.querySelector('[data-lookup-symbol=AAPL]')"));
	browser('press', 'Escape');
	assert.equal(evaluate("document.querySelector('#dlg-input').value"), 'MSFT');
	assert.equal(evaluate('document.activeElement.id'), 'dlg-input');
	button('Look up');
	fill('#lookup-search', 'no-such-company-xyz');
	assert.match(
		evaluate("document.querySelector('[data-symbol-lookup]').textContent"),
		/No companies match/,
	);
	fill('#lookup-search', 'apple');
	click('[data-lookup-symbol=AAPL]');
	assert.equal(evaluate("document.querySelector('#dlg-input').value"), 'AAPL');
	button('OK');
	wait(
		"!!document.querySelector('#dlg') || !document.querySelector('#installer').disabled",
	);
	assert.match(
		evaluate(
			"(document.querySelector('#dlg')?.textContent || '') + document.querySelector('#notices').textContent",
		),
		/APPLE|AAPL/,
	);
	if (evaluate("!!document.querySelector('#dlg')")) button('Cancel');
	idle();
	click('[data-control="110"]'); button('Interest Rate Swaps');
	wait("!!document.querySelector('#game-field-989')");
	fill('#game-field-989', '1,234'); fill('#game-field-993', '2032');
	browser('select', '#game-field-986', 'LONG');
	evaluate("window.previousForm=document.querySelector('[data-game-form]')");
	browser('select', '#game-field-980', 'L');
	wait("document.querySelector('[data-game-form]') !== previousForm");
	assert.equal(evaluate("document.querySelector('#game-field-989').value"), '1,234');
	assert.equal(evaluate("document.querySelector('#game-field-993').value"), '2032');
	assert.equal(evaluate("document.querySelector('#game-field-986').value"), 'LONG');
	button('EXIT'); button('CLOSE'); idle();
	console.log(
		'Forms: grouped setup, computer labels, mobile layout, validation, long-game retry, cancellation, Buy Stock lookup/search/return and swap rate refresh passed.',
	);
} finally {
	browser('close');
}
