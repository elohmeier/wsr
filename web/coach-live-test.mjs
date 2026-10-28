// Opt-in evaluation against a local model, with the real UI, workers and owner documents.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { promisify } from 'node:util';
import { acknowledgeStartupQuote } from './browser-helpers.mjs';

const [
	installer,
	save,
	url = 'http://127.0.0.1:8899/',
	endpoint = 'http://127.0.0.1:8000/v1',
	model = 'deepseek-v4-flash',
	output = 'engine/target/tmp/coach-live',
] = process.argv.slice(2);
assert.ok(
	installer && save,
	'Usage: node web/coach-live-test.mjs INSTALLER SAVE_OR_--new-game [APP_URL] [LOCAL_API_BASE] [MODEL] [OUTPUT_DIR]',
);
assert.ok(
	['127.0.0.1', 'localhost', '[::1]'].includes(new URL(endpoint).hostname),
	'This test sends owner documents only to a local model.',
);
await mkdir(output, { recursive: true });
const run = promisify(execFile);
const session = `wsr-coach-live-${process.pid}`;
const browser = async (...args) => {
	const { stdout } = await run(
		process.env.AGENT_BROWSER_BIN || 'agent-browser',
		['--session', session, '--json', ...args],
		{ timeout: 40000, maxBuffer: 16 * 1024 * 1024 },
	);
	const result = JSON.parse(stdout);
	assert.ok(result.success, JSON.stringify(result.error));
	return result.data;
};
const evaluate = async (js) => (await browser('eval', js)).result;
const wait = (js) => browser('wait', '--fn', js);
const button = (name) =>
	browser('find', 'role', 'button', 'click', '--name', name, '--exact');
const idle = () =>
	wait(
		'!document.querySelector("#dlg") && !document.querySelector("#installer").disabled',
	);
const requests = [];
// ds4-server may omit CORS headers. This test-only proxy forwards its SSE stream unchanged.
const proxy = createServer(async (request, response) => {
	response.setHeader('Access-Control-Allow-Origin', new URL(url).origin);
	response.setHeader('Access-Control-Allow-Headers', '*');
	response.setHeader('Access-Control-Allow-Methods', 'POST, OPTIONS');
	response.setHeader('Cross-Origin-Resource-Policy', 'cross-origin');
	if (request.method === 'OPTIONS') return response.writeHead(204).end();
	const abort = new AbortController();
	response.on('close', () => abort.abort());
	try {
		let body = '';
		for await (const chunk of request) body += chunk;
		const payload = JSON.parse(body);
		requests.push({
			chars: body.length,
			maxTokens: payload.max_tokens ?? payload.max_completion_tokens,
			tools: payload.tools?.length ?? 0,
		});
		const upstream = await fetch(
			`${endpoint.replace(/\/$/, '')}/chat/completions`,
			{
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body,
				signal: abort.signal,
			},
		);
		response.writeHead(upstream.status, {
			'Content-Type':
				upstream.headers.get('content-type') || 'text/event-stream',
		});
		for await (const chunk of upstream.body) response.write(chunk);
		response.end();
	} catch (error) {
		if (!response.destroyed)
			response.end(JSON.stringify({ error: { message: String(error) } }));
	}
});
await new Promise((resolve) => proxy.listen(0, '127.0.0.1', resolve));
const results = [];
try {
	await browser('open', url);
	await browser('set', 'viewport', '1440', '1000');
	await wait('!document.querySelector("#installer").disabled');
	await browser('upload', '#installer', resolve(installer));
	await acknowledgeStartupQuote(browser);
	await idle();
	if (save === '--new-game') {
		await browser('click', '#new-game');
		for (let i = 0; i < 20; i++) {
			await wait(
				'!!document.querySelector("#dlg") || !document.querySelector("#installer").disabled',
			);
			if (!(await evaluate('!!document.querySelector("#dlg")'))) break;
			const dialog =
				await evaluate(`({title:document.querySelector('#dlg-title').textContent,
				prompt:document.querySelector('[for=dlg-input]')?.textContent,
				buttons:[...document.querySelectorAll('#dlg-buttons button')].map(b=>b.textContent)})`);
			await evaluate('window.previousDialog=document.querySelector("#dlg")');
			if (await evaluate('!!document.querySelector("[data-game-form]")')) {
				for (const [id, value] of [[342, 'Coach Tester'], [344, 'Second Player'], [352, '500'], [354, '10'], [356, '2']])
					await browser('fill', '#game-field-' + id, value);
				await button('OK');
			} else if (dialog.prompt) {
				const value =
					dialog.title === 'Number of players'
						? '2'
						: dialog.prompt.startsWith('Player 1')
							? 'Coach Tester'
							: dialog.prompt.startsWith('Player 2')
								? 'Second Player'
								: dialog.prompt.startsWith('Game Length')
									? '10'
									: dialog.prompt.startsWith('Difficulty')
										? '2'
										: dialog.prompt.startsWith('Starting Money')
											? '500'
											: null;
				if (value !== null) await browser('fill', '#dlg-input', value);
				await button('OK');
			} else
				await button(
					dialog.title.endsWith('Number of Players?')
						? 'No'
						: dialog.buttons.includes('Yes')
							? 'Yes'
							: 'OK',
				);
			await wait(
				'document.querySelector("#dlg") !== previousDialog && (!!document.querySelector("#dlg") || !document.querySelector("#installer").disabled)',
			);
		}
	} else {
		await browser('upload', '#save-file', resolve(save));
		await wait(
			'!!document.querySelector("#dlg") || !document.querySelector("#installer").disabled',
		);
		if (await evaluate('!!document.querySelector("#dlg")')) await button('OK');
	}
	await idle();
	if (
		await evaluate(
			'document.querySelector(".wsr-clock")?.dataset.running === "true"',
		)
	) {
		await browser('click', '#ticker-toggle');
		await idle();
	}
	await evaluate(`window.coachRuns=[];window.gameCommands=[];
		window.originalPost=Worker.prototype.postMessage;
		window.watchedWorkers=new WeakSet();
		Worker.prototype.postMessage=function(message,...args){
			if(message.cmd) gameCommands.push(message.cmd);
			if(message.type==='prompt') {
				coachRuns.push({input:message,events:[]});
				if(!watchedWorkers.has(this)) {
					watchedWorkers.add(this);
					this.addEventListener('message',e=>{
						const run=coachRuns.find(r=>r.input.runId===e.data.runId);
						if(run) run.events.push(e.data);
					});
				}
			} return originalPost.call(this,message,...args);
		}`);
	await browser('click', '#open-coach');
	await wait('!!document.querySelector("#coach-provider")');
	await browser('select', '#coach-provider', 'compatible');
	await browser(
		'fill',
		'#coach-base-url',
		`http://127.0.0.1:${proxy.address().port}/v1`,
	);
	await browser('fill', '#coach-model', model);
	await button('Use connection');
	const questions = [
		['strategy', 'Help me build a strategy'],
		[
			'conservative-followup',
			'I want a conservative start, no borrowing. What should I research next, and what would make me reconsider?',
		],
		[
			'unseen-data',
			'Tell me the exact stock price of the selected company next quarter. If you cannot know it, explain what I can check instead.',
		],
	];
	for (const [name, question] of questions) {
		const start = Date.now(),
			firstRequest = requests.length;
		await browser('fill', '#coach-question', question);
		await button('Send');
		console.log(`${name}: waiting for ${model}…`);
		while (
			!(await evaluate('coachRuns.at(-1)?.events.some(e=>e.type==="done")'))
		) {
			assert.ok(
				Date.now() - start < 600000,
				`${name}: evaluation exceeded ten minutes`,
			);
			try {
				await wait('coachRuns.at(-1)?.events.some(e=>e.type==="done")');
			} catch (error) {
				if (!String(error.stdout).includes('Wait timed out')) throw error;
				console.log(
					`${name}: ${Math.round((Date.now() - start) / 1000)}s, ${requests.length - firstRequest} model requests`,
				);
			}
		}
		const captured = await evaluate('coachRuns.at(-1)');
		const texts = new Map();
		for (const event of captured.events)
			if (event.type === 'text') texts.set(event.id, event.text);
		const done = captured.events.find((e) => e.type === 'done');
		const result = {
			name,
			question,
			seconds: (Date.now() - start) / 1000,
			error: done.error,
			requests: requests.slice(firstRequest),
			answers: [...texts.values()],
			tools: captured.events.filter((e) => e.type === 'tool'),
			snapshot: captured.input.snapshot,
			gameCommands: await evaluate('gameCommands'),
		};
		results.push(result);
		await writeFile(
			resolve(output, `${name}.json`),
			JSON.stringify(result, null, 2),
		);
		await browser('screenshot', resolve(output, `${name}.png`));
		assert.equal(done.error, undefined, `${name}: ${done.error}`);
		assert.ok(
			result.answers.at(-1)?.length > 100,
			`${name}: missing explanation`,
		);
		assert.ok(
			result.gameCommands.every((c) => c === 'observe'),
			'Coaching must not perform game actions',
		);
		console.log(
			`${name}: passed, ${result.seconds}s, ${result.requests.length} model requests`,
		);
	}
	await writeFile(
		resolve(output, 'summary.json'),
		JSON.stringify(
			results.map(({ tools, snapshot, ...result }) => result),
			null,
			2,
		),
	);
	console.log(`Live coach results: ${resolve(output)}`);
} catch (error) {
	await browser('screenshot', resolve(output, 'failure.png')).catch(() => {});
	await writeFile(
		resolve(output, 'failure.txt'),
		String(error) +
			'\n' +
			(await evaluate('document.body.innerText').catch(
				() => 'Browser unavailable',
			)),
	);
	throw error;
} finally {
	await browser('close').catch(() => {});
	proxy.closeAllConnections();
	await new Promise((resolve) => proxy.close(resolve));
}
