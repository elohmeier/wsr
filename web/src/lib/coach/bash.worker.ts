import { createShell } from './shell';
import type { Snapshot } from './types';

const scope = self as unknown as {
	onmessage: ((event: MessageEvent) => void) | null;
	postMessage: (value: unknown) => void;
};
let shell: ReturnType<typeof createShell> | undefined;
let running = false;
scope.onmessage = async ({ data: message }: MessageEvent) => {
	if (
		!message ||
		typeof message !== 'object' ||
		!Number.isSafeInteger(message.id)
	)
		return;
	try {
		if (running) throw new Error('A shell command is already running.');
		if (message.type === 'init') {
			shell = createShell(
				message.files as Record<string, string>,
				message.snapshot as Snapshot | null,
			);
			scope.postMessage({ id: message.id, result: { ready: true } });
		} else if (
			message.type === 'exec' &&
			typeof message.command === 'string' &&
			shell
		) {
			running = true;
			try {
				scope.postMessage({
					id: message.id,
					result: await shell.exec(message.command),
				});
			} finally {
				running = false;
			}
		}
	} catch (error) {
		scope.postMessage({
			id: message.id,
			error: error instanceof Error ? error.message : 'Shell failed.',
		});
	}
};
