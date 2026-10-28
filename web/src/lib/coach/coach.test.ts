import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
	createFauxCore,
	fauxAssistantMessage,
	fauxToolCall,
} from '@earendil-works/pi-ai/providers/faux';
import { createShell, queryGame } from './shell';
import { createCoachAgent } from './session';
import { validateConnection } from './types';
import type { CoachEvent, Snapshot } from './types';

const snapshot: Snapshot = {
	sessionId: 'game-a',
	revision: 4,
	capturedAt: '2026-10-27',
	observation: {
		schemaVersion: 1,
		available: true,
		context: {
			playerId: 1,
			playerName: 'Player',
			activeEntityId: 131,
			selectedEntityId: 132,
			gameDate: 'January 1, 2026',
		},
		player: { cash: 20, bankDebt: 5 },
		activeEntity: null,
		portfolios: [
			{
				entityId: 1,
				holdings: [{ id: 131, percentOfCompany: 10, marketValue: 120 }],
			},
		],
		companies: [{ id: 131, name: 'Example', symbol: 'EX', industryId: 3 }],
		industries: [{ id: 3, name: 'Industry' }],
	},
};
const files = {
	'/docs/help/001.txt': 'Borrowing\n\nMargin debt increases leverage.\n',
	'/docs/INDEX.md': 'See /docs/help/001.txt\n',
};

test('real browser shell composes documentation search, read-only CLI and jq', async () => {
	const shell = createShell(files, snapshot);
	const docs = await shell.exec('grep -n leverage /docs/help/001.txt');
	assert.equal(docs.exitCode, 0);
	assert.match(docs.stdout, /3:Margin debt/);
	assert.deepEqual(docs.sources, ['/docs/help/001.txt']);
	const result = await shell.exec(
		"wsr portfolio --json | jq '.holdings[0].marketValue'",
	);
	assert.equal(result.stdout.trim(), '120');
	assert.equal(
		(await shell.exec('echo work > /tmp/work; cat /tmp/work')).stdout,
		'work\n',
	);
	assert.equal(
		(await shell.exec('cat /docs/help/001.txt')).stdout,
		files['/docs/help/001.txt'],
	);
});

test('shell rejects writes, links, network, runtimes and action commands without changing evidence', async () => {
	const shell = createShell(files, snapshot);
	for (const command of [
		'echo altered > /docs/help/001.txt',
		'echo altered > /game/snapshot.json',
		'echo altered > /tmp/../docs/help/001.txt',
		'rm -rf /docs',
		'cp /tmp/.keep /docs/help/001.txt',
		'mv /docs/help/001.txt /tmp/moved',
		'ln -s /docs /tmp/link',
		'ln /docs/help/001.txt /tmp/alias',
		'curl https://example.com',
		'wget https://example.com',
		'python3 -c print(1)',
		'js-exec -c "fetch(\"https://example.com\")"',
		'wsr buy 131',
		'wsr save',
		'wsr portfolio 2',
	]) {
		const result = await shell.exec(command);
		assert.notEqual(result.exitCode, 0, command);
	}
	assert.equal(
		(await shell.exec('cat /docs/help/001.txt')).stdout,
		files['/docs/help/001.txt'],
	);
	assert.deepEqual(
		JSON.parse((await shell.exec('cat /game/snapshot.json')).stdout),
		snapshot,
	);
	assert.doesNotMatch(
		(await shell.exec('env')).stdout,
		/API_KEY|token|password/i,
	);
});

test('execution and output limits stop runaway inspection and leave the shell usable', async () => {
	const shell = createShell(files, snapshot);
	const loop = await shell.exec('while true; do :; done');
	assert.notEqual(loop.exitCode, 0);
	assert.match(loop.stderr, /limit/i);
	const output = await shell.exec("jq -n '[range(0; 1000000)]'");
	assert.notEqual(output.exitCode, 0);
	assert.ok(output.stdout.length <= 16000);
	assert.equal((await shell.exec('echo recovered')).stdout, 'recovered\n');
	const abort = new AbortController();
	abort.abort();
	assert.notEqual(
		(await shell.exec('cat /docs/help/001.txt', abort.signal)).exitCode,
		0,
	);
});

test('jq can filter a complete company list while oversized final responses are marked', async () => {
	const large = {
		...snapshot,
		observation: {
			...snapshot.observation,
			companies: Array.from({ length: 1600 }, (_, id) => ({
				id,
				name: 'Company',
				symbol: `C${id}`,
				industryId: 3,
			})),
		},
	};
	const shell = createShell(files, large);
	const count = await shell.exec('wsr companies | jq length');
	assert.equal(count.stdout.trim(), '1600');
	assert.equal(count.exitCode, 0);
	const all = await shell.exec('wsr companies');
	assert.notEqual(all.exitCode, 0);
	assert.match(all.stderr, /truncated/i);
	assert.ok(all.stdout.length + all.stderr.length <= 16000);
});

test('CLI exposes pending screen context but no unseen research or other entity portfolios', () => {
	const pending = {
		...snapshot,
		pendingDialog: true,
		screen: { kind: 'report', title: 'Research', lines: ['Visible facts'] },
	};
	assert.deepEqual(queryGame(pending, ['report', 'current']), pending.screen);
	assert.throws(
		() => queryGame(snapshot, ['portfolio', '2']),
		/Only this player/,
	);
	assert.throws(
		() => queryGame(snapshot, ['history', '131']),
		/Open this company/,
	);
	assert.throws(
		() => queryGame(snapshot, ['company', '131', '--hidden']),
		/Unexpected/,
	);
	assert.equal(
		(queryGame(snapshot, ['company', 'EX']) as { id: number }).id,
		131,
	);
});

test('Pi faux provider performs real shell research before explaining with a source', async () => {
	const faux = createFauxCore({ tokensPerSecond: 100000 });
	faux.setResponses([
		fauxAssistantMessage(
			[
				fauxToolCall('bash', {
					command: 'grep -n leverage /docs/help/001.txt; wsr player',
				}),
			],
			{ stopReason: 'toolUse' },
		),
		(context) => {
			const evidence = JSON.stringify(context.messages);
			assert.match(evidence, /Margin debt increases leverage/);
			assert.match(evidence, /bankDebt/);
			assert.doesNotMatch(evidence, /apiKey/);
			return fauxAssistantMessage(
				'Borrowing increases leverage. [Borrowing](/docs/help/001.txt#L3-L3)',
			);
		},
	]);
	const events: CoachEvent[] = [];
	const shell = createShell(files, snapshot);
	const { agent } = createCoachAgent({
		model: faux.getModel(),
		streamFn: faux.streamSimple,
		snapshot,
		history: [],
		exec: (command, signal) => shell.exec(command, signal),
		emit: (event) => events.push(event),
	});
	await agent.prompt('Explain borrowing.');
	assert.equal(faux.state.callCount, 2);
	assert.ok(
		events.some(
			(event) =>
				event.type === 'sources' && event.paths.includes('/docs/help/001.txt'),
		),
	);
	assert.ok(
		events.some(
			(event) => event.type === 'text' && event.text.includes('[Borrowing]'),
		),
	);
});

test('conversation history keeps speaker identity and marks older snapshots', async () => {
	const faux = createFauxCore({ tokensPerSecond: 100000 });
	faux.setResponses([
		(context) => {
			const conversation = context.messages.filter(
				(message) => message.role !== 'system',
			);
			assert.equal(conversation[0].role, 'user');
			assert.equal(conversation[1].role, 'assistant');
			assert.match(JSON.stringify(conversation[1]), /Earlier snapshot 3/);
			assert.match(JSON.stringify(context.messages[0]), /revision\\\":4/);
			return fauxAssistantMessage('Let us recheck current debt.');
		},
	]);
	const run = createCoachAgent({
		model: faux.getModel(),
		streamFn: faux.streamSimple,
		snapshot,
		history: [
			{
				id: 'q',
				role: 'user',
				text: 'Explain debt.',
				sources: [],
				revision: 3,
				gameDate: 'Earlier date',
			},
			{
				id: 'a',
				role: 'assistant',
				text: 'Earlier debt was 5.',
				sources: [],
				revision: 3,
				gameDate: 'Earlier date',
			},
		],
		exec: async () => {
			throw new Error('No tool expected');
		},
		emit: () => {},
	});
	await run.agent.prompt('What about now?');
	assert.equal(faux.state.callCount, 1);
	assert.equal(run.agent.state.errorMessage, undefined);
});

test('Pi research is bounded even when the model keeps asking for tools', async () => {
	const faux = createFauxCore({ tokensPerSecond: 100000 });
	faux.setResponses(
		Array.from({ length: 20 }, () =>
			fauxAssistantMessage([fauxToolCall('bash', { command: 'wsr context' })], {
				stopReason: 'toolUse',
			}),
		),
	);
	const shell = createShell(files, snapshot);
	const run = createCoachAgent({
		model: faux.getModel(),
		streamFn: faux.streamSimple,
		snapshot,
		history: [],
		exec: (command, signal) => shell.exec(command, signal),
		emit: () => {},
	});
	await run.agent.prompt('Keep researching.');
	assert.ok(run.limited);
	assert.ok(faux.state.callCount <= 6);
});

test('provider configuration accepts secure endpoints and localhost without embedding credentials in URLs', () => {
	const connection = {
		provider: 'compatible' as const,
		model: 'test',
		baseUrl: 'http://127.0.0.1:1234/v1',
		apiKey: '',
	};
	assert.doesNotThrow(() => validateConnection(connection));
	for (const baseUrl of [
		'http://example.com/v1',
		'https://key@example.com/v1',
		'javascript:alert(1)',
		'https://example.com/v1?key=secret',
	]) {
		assert.throws(() => validateConnection({ ...connection, baseUrl }));
	}
});
