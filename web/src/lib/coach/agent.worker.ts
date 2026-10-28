import { streamSimple } from '@earendil-works/pi-ai/api/openai-completions';
import type { Model } from '@earendil-works/pi-ai';
import { createCoachAgent } from './session';
import { MAX_QUESTION, validateConnection } from './types';
import type { ChatMessage, CoachEvent, Connection, Snapshot } from './types';
import type { ShellResult } from './session';

const scope = self as unknown as {
	onmessage: ((event: MessageEvent) => void) | null;
	postMessage: (value: unknown) => void;
};
let active: ReturnType<typeof createCoachAgent> | undefined;
let shell: Worker | undefined;
let cancelShell: (() => void) | undefined;
let running = false;
let sequence = 0;

scope.onmessage = async ({ data: message }: MessageEvent) => {
	if (!message || typeof message !== 'object') return;
	if (message.type === 'stop') {
		active?.agent.abort();
		cancelShell?.();
		shell?.terminate();
		return;
	}
	if (
		message.type !== 'prompt' ||
		running ||
		typeof message.question !== 'string'
	)
		return;
	const runId = message.runId as number;
	const connection = message.connection as Connection;
	const redact = (text: string) =>
		connection.apiKey
			? text.replaceAll(connection.apiKey, '[credential removed]')
			: text;
	const emit = (event: CoachEvent) =>
		scope.postMessage({
			runId,
			...event,
			...(event.type === 'done' && event.error
				? { error: redact(event.error) }
				: {}),
		});
	let deadline: ReturnType<typeof setTimeout> | undefined;
	let timedOut = false;
	running = true;
	try {
		validateConnection(connection);
		if (!message.question.trim() || message.question.length > MAX_QUESTION)
			throw new Error('Enter a question of up to 4,000 characters.');
		const model: Model<'openai-completions'> = {
			id: connection.model.trim(),
			name: connection.model.trim(),
			provider:
				connection.provider === 'openrouter'
					? 'openrouter'
					: 'coach-compatible',
			api: 'openai-completions',
			baseUrl:
				connection.provider === 'openrouter'
					? 'https://openrouter.ai/api/v1'
					: connection.baseUrl.replace(/\/$/, ''),
			reasoning: false,
			input: ['text'],
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
			contextWindow: 32768,
			maxTokens: 2048,
		};
		shell = new Worker(new URL('./bash.worker.ts', import.meta.url), {
			type: 'module',
		});
		const request = <T>(
			body: Record<string, unknown>,
			signal?: AbortSignal,
		): Promise<T> =>
			new Promise((resolve, reject) => {
				const id = ++sequence;
				const cleanup = () => {
					clearTimeout(timer);
					shell?.removeEventListener('message', reply);
					shell?.removeEventListener('error', failed);
					signal?.removeEventListener('abort', cancelled);
					cancelShell = undefined;
				};
				const cancelled = () => {
					cleanup();
					shell?.terminate();
					reject(new Error('Stopped.'));
				};
				const failed = () => {
					cleanup();
					reject(new Error('The inspection worker stopped. Try again.'));
				};
				const reply = (event: MessageEvent) => {
					if (event.data?.id !== id) return;
					cleanup();
					if (event.data.error) reject(new Error(event.data.error));
					else resolve(event.data.result as T);
				};
				const timer = setTimeout(() => {
					cleanup();
					shell?.terminate();
					reject(new Error('Inspection timed out. Use a smaller query.'));
				}, 6000);
				cancelShell = cancelled;
				if (signal?.aborted) {
					cancelled();
					return;
				}
				signal?.addEventListener('abort', cancelled, { once: true });
				shell!.addEventListener('message', reply);
				shell!.addEventListener('error', failed);
				shell!.postMessage({ id, ...body });
			});
		await request({
			type: 'init',
			files: message.files,
			snapshot: message.snapshot,
		});
		active = createCoachAgent({
			model,
			snapshot: message.snapshot as Snapshot | null,
			history: message.history as ChatMessage[],
			emit,
			streamFn: (_model, context, options) =>
				streamSimple(model, context, {
					...options,
					apiKey: connection.apiKey || 'local',
					maxTokens: 1800,
					maxRetryDelayMs: 0,
				}),
			exec: (command, signal) =>
				request<ShellResult>({ type: 'exec', command }, signal),
		});
		deadline = setTimeout(() => {
			timedOut = true;
			active?.agent.abort();
			cancelShell?.();
		}, 90000);
		await active.agent.prompt(message.question.trim());
		emit({
			type: 'done',
			error: timedOut
				? 'The coach reached its 90-second limit. Ask a more focused question to continue.'
				: (active.agent.state.errorMessage ??
					(active.limited
						? 'Research limit reached. Ask a more focused follow-up to continue.'
						: undefined)),
		});
	} catch (error) {
		emit({
			type: 'done',
			error:
				error instanceof Error
					? error.message
					: 'The coach could not complete this question.',
		});
	} finally {
		clearTimeout(deadline);
		shell?.terminate();
		shell = undefined;
		active = undefined;
		cancelShell = undefined;
		running = false;
	}
};
