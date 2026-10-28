import { acknowledgeStartupQuote } from './browser-helpers.mjs';
// Real browser, Pi transport, nested shell worker and WASM; the model endpoint is local and scripted.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { writeFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';

const [installer, save, url = 'http://127.0.0.1:8899/'] = process.argv.slice(2);
if (!installer || !save)
	throw new Error(
		'Provide the owner installer and a complete unfinished human-player save (browser_fixtures GAME49.DAT is suitable).',
	);
const session =
	process.env.WSR_COACH_BROWSER_SESSION || `wsr-coach-${process.pid}`;
const run = promisify(execFile);
const browser = async (...args) => {
	const { stdout } = await run(
		process.env.AGENT_BROWSER_BIN || 'agent-browser',
		['--session', session, '--json', ...args],
		{ timeout: 40000, maxBuffer: 8 * 1024 * 1024 },
	);
	const result = JSON.parse(stdout);
	assert.ok(result.success, JSON.stringify(result.error));
	return result.data;
};
const evaluate = async (js) => (await browser('eval', js)).result;
const wait = (js) => browser('wait', '--fn', js);
const click = (selector) => browser('click', selector);
const fill = (selector, value) => browser('fill', selector, String(value));
const button = (name) =>
	browser('find', 'role', 'button', 'click', '--name', name, '--exact');
const idle = () =>
	wait(
		"!document.querySelector('#dlg') && !document.querySelector('#installer').disabled",
	);
async function checkMobileCoach(width, height) {
	await browser('set', 'viewport', String(width), String(height));
	await wait("!!document.querySelector('.coach-sheet')");
	const geometry = await evaluate(`(() => {
		const bounds = selector => {
			const element = document.querySelector(selector), r = element.getBoundingClientRect();
			return { left:r.left, top:r.top, right:r.right, bottom:r.bottom, height:r.height };
		};
		return { width:innerWidth, height:innerHeight, pageWidth:document.documentElement.scrollWidth,
			sheet:bounds('.coach-sheet'), composer:bounds('.coach-composer'),
			question:bounds('#coach-question'), thread:bounds('.coach-thread') };
	})()`);
	for (const key of ['sheet', 'composer', 'question']) {
		const rect = geometry[key];
		assert.ok(rect.left >= -1 && rect.top >= -1 && rect.right <= width + 1 && rect.bottom <= height + 1,
			`${width}×${height}: ${key} is outside the viewport: ${JSON.stringify(geometry)}`);
	}
	assert.ok(geometry.pageWidth <= width, `Horizontal page overflow at ${width}×${height}`);
	assert.ok(geometry.thread.height >= 80, `The conversation has no usable space at ${width}×${height}`);
}
const requests = [];
const sockets = new Set();
const server = createServer(async (request, response) => {
	response.setHeader('Access-Control-Allow-Origin', new URL(url).origin);
	response.setHeader('Access-Control-Allow-Headers', '*');
	response.setHeader('Access-Control-Allow-Methods', 'POST, OPTIONS');
	response.setHeader('Cross-Origin-Resource-Policy', 'cross-origin');
	if (request.method === 'OPTIONS') {
		response.writeHead(204).end();
		return;
	}
	let text = '';
	for await (const chunk of request) text += chunk;
	const body = JSON.parse(text);
	requests.push(body);
	const lastUser = body.messages.findLastIndex(
		(message) => message.role === 'user',
	);
	const userContent = body.messages[lastUser]?.content ?? '';
	const question =
		typeof userContent === 'string'
			? userContent
			: userContent.map((part) => part.text ?? '').join('\n');
	response.writeHead(200, {
		'Content-Type': 'text/event-stream',
		'Cache-Control': 'no-cache',
	});
	const send = (delta, finish_reason = null) =>
		response.write(
			`data: ${JSON.stringify({ id: 'local-response', object: 'chat.completion.chunk', created: 1, model: 'coach-test', choices: [{ index: 0, delta, finish_reason }] })}\n\n`,
		);
	send({ role: 'assistant', content: '' });
	if (question.includes('mobile viewport')) {
		send({ content: 'Mobile viewport response.\n\n' + 'Read the explanation and ask a follow-up.\n\n'.repeat(24) });
		send({}, 'stop');
		response.end('data: [DONE]\n\n');
		return;
	}
	if (question.includes('stop this response')) {
		send({ content: 'This response can be stopped.' });
		return;
	}
	if (question.includes('long explanation')) {
		for (let i = 1; i <= 80 && !response.destroyed; i++) {
			send({
				content: `Risk discussion paragraph ${i}: compare available evidence and consider how debt can magnify a loss.\n\n`,
			});
			await new Promise((resolve) => setTimeout(resolve, 200));
		}
		send({}, 'stop');
		response.end('data: [DONE]\n\n');
		return;
	}
	const tool = body.messages
		.slice(lastUser + 1)
		.find((message) => message.role === 'tool');
	if (!tool) {
		send({
			tool_calls: [
				{
					index: 0,
					id: `read-${requests.length}`,
					type: 'function',
					function: {
						name: 'bash',
						arguments: JSON.stringify({
							command:
								"grep '/docs/tutorials/' /docs/INDEX.md; wsr context; wsr player",
						}),
					},
				},
			],
		});
		send({}, 'tool_calls');
	} else {
		const path = JSON.stringify(tool.content).match(
			/\/docs\/tutorials\/[^\\\s:]+\.txt/,
		)?.[0];
		assert.ok(
			path,
			'The shell must actually read the imported tutorial index.',
		);
		const reply = `Concentrated holdings increase exposure to one company. Compare your cash and bank debt before deciding. [Your tutorial](${path}#L3-L3)`;
		// Index discovery does not count as reading a source. Read the actual tutorial before citing it.
		if (
			!body.messages
				.slice(lastUser + 1)
				.some(
					(message) =>
						message.role === 'assistant' &&
						JSON.stringify(message).includes('cat /docs/tutorials/'),
				)
		) {
			send({
				tool_calls: [
					{
						index: 0,
						id: `source-${requests.length}`,
						type: 'function',
						function: {
							name: 'bash',
							arguments: JSON.stringify({ command: `cat ${path}` }),
						},
					},
				],
			});
			send({}, 'tool_calls');
		} else {
			send({ content: reply });
			send({}, 'stop');
		}
	}
	response.end('data: [DONE]\n\n');
});
server.on('connection', (socket) => {
	sockets.add(socket);
	socket.on('close', () => sockets.delete(socket));
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const directory = await mkdtemp(join(tmpdir(), 'wsr-coach-'));
const tutorial = join(directory, 'tutorial.html');
await writeFile(
	tutorial,
	'<h1>Concentrated holdings amplify one company’s risk.</h1><p>Compare debt with cash before deciding.</p><script>window.coachInjection="untrustedExecutableMarker"</script><img src="https://example.invalid/should-never-fetch" alt="Example">',
);

try {
	await browser('open', url);
	await browser('set', 'viewport', '1440', '1000');
	await wait("!document.querySelector('#installer').disabled");
	await browser('upload', '#installer', resolve(installer));
	await acknowledgeStartupQuote(browser);
	await idle();
	await browser('upload', '#save-file', resolve(save));
	await wait(
		"!!document.querySelector('#dlg') || !document.querySelector('#installer').disabled",
	);
	if (await evaluate("!!document.querySelector('#dlg')")) await button('OK');
	await idle();
	assert.ok(
		await evaluate("!!document.querySelector('#desk')"),
		await evaluate("document.querySelector('#error')?.textContent"),
	);
	console.log('Coach browser: loaded game fixture.');
	if (
		await evaluate(
			"document.querySelector('.wsr-clock')?.dataset.running === 'true'",
		)
	) {
		await click('#ticker-toggle');
		await idle();
	}
	await click('#open-coach');
	await wait("!!document.querySelector('#coach-provider')");
	await browser('select', '#coach-provider', 'compatible');
	await fill('#coach-base-url', `http://127.0.0.1:${server.address().port}/v1`);
	await fill('#coach-model', 'coach-test');
	await fill('#coach-api-key', 'not-a-real-key');
	await button('Use connection');
	await browser('upload', '#coach-tutorial', tutorial);
	console.log('Coach browser: configured local model and tutorial.');
	await evaluate(
		'window.coachCommands=[];window.coachPosted=[];window.coachOriginalPost=Worker.prototype.postMessage;Worker.prototype.postMessage=function(message,...args){if(message.cmd){coachCommands.push(message.cmd);coachPosted.push({cmd:message.cmd,action:message.action,text:message.text?Array.from(message.text):null})}return coachOriginalPost.call(this,message,...args)}',
	);
	await fill(
		'#coach-question',
		'Explain concentrated holdings using my tutorial and game state.',
	);
	// IME composition and Shift+Enter must leave the question unsent.
	await evaluate(
		"document.querySelector('#coach-question').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',isComposing:true,bubbles:true})); document.querySelector('#coach-question').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',shiftKey:true,bubbles:true}))",
	);
	assert.equal(requests.length, 0);
	await button('Send');
	console.log('Coach browser: waiting for the Pi tool loop.');
	await wait(
		"document.querySelector('#learning-coach')?.textContent.includes('Compare your cash and bank debt') && !document.querySelector('[aria-label=\"Stop response\"]')",
	);
	assert.equal(requests.length, 3);
	assert.ok(
		requests[1].messages.some(
			(message) =>
				message.role === 'tool' && message.content.includes('playerId'),
		),
	);
	assert.ok(!JSON.stringify(requests).includes('not-a-real-key'));
	assert.ok(!JSON.stringify(requests).includes('option_recs'));
	assert.ok(!JSON.stringify(requests).includes('untrustedExecutableMarker'));
	assert.ok(!(await evaluate('window.coachInjection')));
	assert.ok(
		(await evaluate('coachCommands')).every((command) => command === 'observe'),
	);
	await browser('focus', '.coach-citation');
	await browser('press', 'Enter');
	await wait("!!document.querySelector('.coach-source-text')");
	assert.match(
		await evaluate(
			"document.querySelector('.coach-source-text').value || document.querySelector('.coach-source-text').textContent",
		),
		/Concentrated holdings/,
	);
	await button('Close source');
	console.log(
		'Coach browser: verified state access and local source citation.',
	);
	assert.ok(
		await evaluate(
			"document.querySelector('.app-shell').getBoundingClientRect().right <= document.querySelector('.coach-dock').getBoundingClientRect().left",
		),
	);
	assert.equal(await evaluate('document.documentElement.scrollWidth'), 1440);
	await fill(
		'#coach-question',
		'Give a long explanation for the scrolling check.',
	);
	await button('Send');
	await wait(
		"document.querySelector('#learning-coach').textContent.includes('Risk discussion paragraph 5:')",
	);
	await evaluate(
		"window.coachViewport=document.querySelector('[data-slot=\"message-scroller-viewport\"]');coachViewport.dispatchEvent(new WheelEvent('wheel',{deltaY:-700,bubbles:true}));coachViewport.scrollTop=0;coachViewport.dispatchEvent(new Event('scroll'));window.coachEarlierPosition=coachViewport.scrollTop",
	);
	await wait(
		"document.querySelector('#learning-coach').textContent.includes('Risk discussion paragraph 80:') && !document.querySelector('[aria-label=\"Stop response\"]')",
	);
	assert.ok(
		await evaluate('Math.abs(coachViewport.scrollTop-coachEarlierPosition)<4'),
		"Streaming must preserve the reader's earlier scroll position.",
	);
	await click('[aria-label="Jump to latest"]');
	await wait('coachViewport.scrollTop>500');
	console.log('Coach browser: verified streaming scroll and Jump to latest.');
	await fill('#coach-question', 'stop this response');
	await button('Send');
	await wait('!!document.querySelector(\'[aria-label="Stop response"]\')');
	await click('[aria-label="Stop response"]');
	assert.ok(
		!(await evaluate(
			'!!document.querySelector(\'[aria-label="Stop response"]\')',
		)),
	);
	await button('Close learning coach');
	console.log('Coach browser: verified cancellation.');
	// Engine dialogs keep their own actions; the coach is opened from the desk menu.
	assert.equal(
		await evaluate("document.querySelector('#symbol').readOnly"),
		false,
		'The human fixture must allow company selection.',
	);
	await fill('#symbol', 'AAPL');
	await button('Go');
	await idle();
	await wait(
		"document.querySelector('#selected-name')?.textContent.toUpperCase().includes('APPLE')",
	);
	console.log(
		'Coach browser: selected company:',
		await evaluate("document.querySelector('#selected-name')?.textContent"),
	);
	await click('[data-control="107"]');
	await wait("!!document.querySelector('#dlg')");
	assert.equal(
		await evaluate(
			"document.querySelector('#dlg').textContent.includes('Explain this')",
		),
		false,
		'Game dialogs must not include a coach shortcut.',
	);
	await button('Advanced Option Trading');
	await wait("!!document.querySelector('.option-grid')");
	await fill('[aria-label="Line 1 strike"]', '123');
	assert.equal(
		await evaluate(
			"document.querySelector('#dlg').textContent.includes('Explain this')",
		),
		false,
		'Order dialogs must not include a coach shortcut.',
	);
	assert.equal(
		await evaluate(
			'document.querySelector(\'[aria-label="Line 1 strike"]\').value',
		),
		'123',
	);
	assert.ok(await evaluate("!!document.querySelector('.option-grid')"));
	await button('Close');
	await button('CLOSE');
	await idle();
	await browser('upload', '#save-file', resolve(save));
	await idle();
	await click('#open-coach');
	await wait("!!document.querySelector('#learning-coach')");
	assert.ok(
		!(await evaluate(
			"document.querySelector('#learning-coach').textContent.includes('Compare your cash and bank debt')",
		)),
		'Loading a game must clear the previous conversation.',
	);
	assert.equal(await evaluate('Object.keys(localStorage).length'), 0);
	await button('Close learning coach');
	console.log('Coach browser: verified dialog actions and game-scope reset.');
	// Open a fresh compact view, including connection setup. This also verifies that
	// credentials/conversation are not restored after navigation.
	await browser('set', 'viewport', '390', '844');
	await browser('set', 'media', 'dark', 'reduced-motion');
	await browser('open', url);
	await acknowledgeStartupQuote(browser);
	await wait("!document.querySelector('#installer').disabled");
	await click('#open-coach');
	await wait("!!document.querySelector('.coach-sheet')");
	assert.ok(
		await evaluate(
			"document.querySelector('.coach-sheet').getBoundingClientRect().width <= innerWidth",
		),
	);
	assert.ok(
		await evaluate(
			"document.querySelector('#coach-question').getBoundingClientRect().bottom <= innerHeight",
		),
	);
	assert.ok(
		await evaluate(
			"document.querySelector('#coach-question').getBoundingClientRect().right <= innerWidth",
		),
	);
	assert.equal(
		await evaluate("document.querySelector('#coach-api-key').value"),
		'',
	);
	await browser('select', '#coach-provider', 'compatible');
	for (const [width, height] of [[390, 844], [375, 667], [320, 568], [844, 390], [390, 420]])
		await checkMobileCoach(width, height);
	await fill('#coach-base-url', `http://127.0.0.1:${server.address().port}/v1`);
	await fill('#coach-model', 'coach-test');
	await fill('#coach-api-key', 'not-a-real-key');
	await button('Use connection');
	await fill('#coach-question', 'Check the mobile viewport.');
	await button('Send');
	await wait("document.querySelector('#learning-coach').textContent.includes('Mobile viewport response.') && !document.querySelector('[aria-label=\"Stop response\"]')");
	for (const [width, height] of [[390, 844], [375, 667], [320, 568], [844, 390], [390, 420]])
		await checkMobileCoach(width, height);
	assert.ok(await evaluate("document.querySelector('[data-slot=\"message-scroller-viewport\"]').scrollHeight > document.querySelector('[data-slot=\"message-scroller-viewport\"]').clientHeight"),
		'The mobile conversation must scroll independently of the composer');
	await button('Close learning coach');
	console.log(
		'Coach browser: real Pi transport and bash worker, source citations, state inspection, no player actions, IME, cancellation, dialogs without coach shortcuts, mobile and game-scope reset passed.',
	);
} catch (error) {
	console.error(
		'Scroller geometry:',
		await evaluate(
			"[...document.querySelectorAll('[data-slot^=\"message-scroller\"]')].map(e=>({slot:e.dataset.slot,h:e.clientHeight,scroll:e.scrollTop,total:e.scrollHeight,visible:getComputedStyle(e).visibility,text:e.textContent.length,attrs:[...e.attributes].filter(a=>a.name.startsWith('data-')).map(a=>[a.name,a.value])}))",
		).catch(() => 'unavailable'),
	);
	console.error(
		'Coach text:',
		await evaluate(
			"document.querySelector('#learning-coach')?.innerText",
		).catch(() => 'unavailable'),
	);
	console.error(
		'Browser errors:',
		await browser('errors').catch(() => 'unavailable'),
	);
	console.error('Model requests:', requests.length);
	console.error(
		'Game dialog:',
		await evaluate("document.querySelector('#dlg')?.innerText").catch(
			() => 'unavailable',
		),
	);
	console.error(
		'Game controls:',
		await evaluate(
			"({readonly:document.querySelector('#symbol')?.readOnly,value:document.querySelector('#symbol')?.value,selected:document.querySelector('#selected-name')?.textContent,error:document.querySelector('#error')?.textContent,notices:document.querySelector('#notices')?.textContent,posted:window.coachPosted,active:document.activeElement?.id})",
		).catch(() => 'unavailable'),
	);
	console.error(
		'Inspection result:',
		JSON.stringify(
			requests[1]?.messages.filter((message) => message.role === 'tool'),
		).slice(0, 2500),
	);
	throw error;
} finally {
	await browser('close').catch(() => {});
	for (const socket of sockets) socket.destroy();
	await new Promise((resolve) => server.close(resolve));
	await rm(directory, { recursive: true, force: true });
}
