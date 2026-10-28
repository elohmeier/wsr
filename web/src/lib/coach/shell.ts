import { Bash, InMemoryFs, defineCommand } from 'just-bash/browser';
import type { IFileSystem } from 'just-bash/browser';
import type { Json, Snapshot } from './types';
import { MAX_COMMAND_OUTPUT } from './types';

const help = `Read-only Wall Street Raider inspection. Every command uses one frozen snapshot.
wsr context | player | active | portfolio [entityId] | companies | company <id-or-symbol>
wsr industries | industry <id> | economy | news | report current | history <id> | help
Append --json for JSON output (the default). No action commands exist.
Company research and chart history are available only from a currently displayed report/chart.
Read wsr context for currency and money units; holdings are percent of the COMPANY.
`;

export function queryGame(snapshot: Snapshot | null, args: string[]): unknown {
	const words = args.filter((arg) => arg !== '--json');
	const [command = 'help', arg] = words;
	if (
		words.length >
		(['company', 'industry', 'portfolio', 'history', 'report'].includes(command)
			? 2
			: 1)
	)
		throw new Error('Unexpected arguments. Run wsr help.');
	if (command === 'help' || command === '--help') return { help };
	if (!snapshot)
		throw new Error(
			'No game snapshot is available. You can still search /docs.',
		);
	const data = snapshot.observation;
	if (command === 'context')
		return {
			sessionId: snapshot.sessionId,
			revision: snapshot.revision,
			capturedAt: snapshot.capturedAt,
			pendingDialog: !!snapshot.pendingDialog,
			context: data.context,
			units: data.units,
			limitations: data.limitations,
			screen: snapshot.screen,
		};
	if (command === 'report' && arg === 'current')
		return snapshot.screen ?? { unavailable: 'No report is open.' };
	if (!data.available)
		throw new Error(data.reason ?? 'No human player is available.');
	switch (command) {
		case 'player':
			return data.player;
		case 'active':
			return (
				data.activeEntity ?? {
					unavailable: 'The active entity is not controlled by this player.',
				}
			);
		case 'portfolio': {
			const id = arg ? Number(arg) : data.context?.playerId;
			const portfolio = data.portfolios?.find((p) => p.entityId === id);
			if (!portfolio)
				throw new Error(
					'Only this player and the active controlled company are available.',
				);
			return portfolio;
		}
		case 'companies':
			return data.companies;
		case 'company': {
			const company = data.companies?.find(
				(c) =>
					String(c.id) === arg || c.symbol.toLowerCase() === arg?.toLowerCase(),
			);
			if (!company)
				throw new Error(
					'Unknown company. Use wsr companies to find its ID or symbol.',
				);
			return company;
		}
		case 'industries':
			return data.industries;
		case 'industry': {
			const industry = data.industries?.find((i) => String(i.id) === arg);
			if (!industry) throw new Error('Unknown industry. Use wsr industries.');
			return {
				...industry,
				companies: data.companies?.filter((c) => c.industryId === industry.id),
			};
		}
		case 'economy':
			return data.economy;
		case 'news':
			return data.news;
		case 'history': {
			const screen = snapshot.screen as Record<string, Json> | undefined;
			if (screen?.kind !== 'chart' || String(screen.entity) !== arg)
				throw new Error(
					'Open this company’s chart in the game to share its history.',
				);
			return screen;
		}
		default:
			throw new Error(`Unknown read-only command: ${command}. Run wsr help.`);
	}
}

/** No links can be created. Every mutating filesystem operation is bounded to /tmp. */
function protect(fs: InMemoryFs, reads: Set<string>): IFileSystem {
	const writable = (path: string) => {
		const resolved = fs.resolvePath('/', path);
		if (resolved !== '/tmp' && !resolved.startsWith('/tmp/'))
			throw new Error(`EROFS: read-only path: ${resolved}`);
	};
	const readMethods = new Set(['readFile', 'readFileBytes', 'readFileBuffer']);
	const writeMethods = new Set([
		'writeFile',
		'appendFile',
		'mkdir',
		'createExclusive',
		'rm',
		'chmod',
		'utimes',
	]);
	return new Proxy(fs, {
		get(target, key) {
			const method = Reflect.get(target, key, target);
			if (typeof method !== 'function') return method;
			if (key === 'symlink' || key === 'link')
				return async () => {
					throw new Error('EPERM: links are disabled.');
				};
			if (
				typeof key === 'string' &&
				(writeMethods.has(key) || key === 'cp' || key === 'mv')
			) {
				return (...args: unknown[]) => {
					writable(String(args[key === 'cp' ? 1 : 0]));
					if (key === 'mv') writable(String(args[1]));
					return method.apply(target, args);
				};
			}
			if (typeof key === 'string' && readMethods.has(key))
				return async (...args: unknown[]) => {
					const value = await method.apply(target, args);
					const path = fs.resolvePath('/', String(args[0]));
					if (path.startsWith('/docs/')) reads.add(path);
					return value;
				};
			return method.bind(target);
		},
	});
}

export function createShell(
	files: Record<string, string>,
	snapshot: Snapshot | null,
) {
	const reads = new Set<string>();
	const memory = new InMemoryFs(
		{
			...files,
			'/game/snapshot.json': JSON.stringify(snapshot),
			'/tmp/.keep': '',
		},
		{ maxTotalBytes: 12 * 1024 * 1024 },
	);
	const bash = new Bash({
		fs: protect(memory, reads),
		cwd: '/tmp',
		env: { HOME: '/tmp', TMPDIR: '/tmp' },
		commands: [
			'cat',
			'ls',
			'grep',
			'find',
			'head',
			'tail',
			'sed',
			'awk',
			'jq',
			'sort',
			'uniq',
			'wc',
			'cut',
			'tr',
			'echo',
			'printf',
			'pwd',
			'mkdir',
			'tee',
			'nl',
			'true',
			'false',
		],
		executionLimitProfile: 'hardened',
		executionLimits: {
			maxExecutionTimeMs: 3000,
			maxCommandCount: 1000,
			maxLoopIterations: 2000,
			// Pipes may process a whole company list; only the final tool result reaches the model.
			maxOutputSize: 2 * 1024 * 1024,
			maxStringLength: 2 * 1024 * 1024,
			maxSourceBytes: 8000,
			maxFileSystemBytes: 12 * 1024 * 1024,
		},
		customCommands: [
			defineCommand('wsr', async (args) => {
				try {
					return {
						stdout: JSON.stringify(queryGame(snapshot, args), null, 2) + '\n',
						stderr: '',
						exitCode: 0,
					};
				} catch (error) {
					return {
						stdout: '',
						stderr:
							String(error instanceof Error ? error.message : error) + '\n',
						exitCode: 2,
					};
				}
			}),
		],
	});
	return {
		async exec(command: string, signal?: AbortSignal) {
			if (command.length > 8000)
				throw new Error('The shell command is too large.');
			try {
				const result = await bash.exec(command, { signal });
				const truncated =
					result.stdout.length + result.stderr.length > MAX_COMMAND_OUTPUT;
				return {
					stdout: truncated
						? result.stdout.slice(0, MAX_COMMAND_OUTPUT - 160)
						: result.stdout,
					stderr: truncated
						? 'Output truncated at 16,000 characters. Refine the query with grep, head or jq.'
						: result.stderr,
					exitCode: truncated ? 2 : result.exitCode,
					sources: [...reads].filter(
						(p) => p in files && p !== '/docs/INDEX.md',
					),
				};
			} catch (error) {
				return {
					stdout: '',
					stderr: error instanceof Error ? error.message : 'Inspection failed.',
					exitCode: 1,
					sources: [...reads],
				};
			}
		},
	};
}
