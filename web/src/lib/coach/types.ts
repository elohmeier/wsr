export type Json =
	null | boolean | number | string | Json[] | { [key: string]: Json };
export interface Observation {
	schemaVersion: number;
	available: boolean;
	reason?: string;
	context?: {
		playerId: number;
		playerName: string;
		activeEntityId: number;
		selectedEntityId: number;
		gameDate: string;
		[key: string]: Json;
	};
	player?: Json;
	activeEntity?: Json;
	portfolios?: { entityId: number; [key: string]: Json }[];
	companies?: {
		id: number;
		name: string;
		symbol: string;
		industryId: number;
		[key: string]: Json;
	}[];
	industries?: { id: number; name: string }[];
	economy?: Json;
	news?: Json;
	units?: Json;
	limitations?: string[];
}
export interface Snapshot {
	sessionId: string;
	revision: number;
	capturedAt: string;
	observation: Observation;
	screen?: Json;
	pendingDialog?: boolean;
}
export interface Source {
	path: string;
	title: string;
	text: string;
	mode: 'help' | 'glossary' | 'manual' | 'tutorial';
	anchor?: string;
	version?: string;
}
export interface Connection {
	provider: 'openrouter' | 'compatible';
	model: string;
	baseUrl: string;
	apiKey: string;
}
export interface ChatMessage {
	id: string;
	role: 'user' | 'assistant';
	text: string;
	sources: string[];
	revision: number;
	gameDate: string;
}
export interface ToolEntry {
	id: string;
	command: string;
	status: 'running' | 'done' | 'error';
	output: string;
}
export type CoachEvent =
	| { type: 'text'; id: string; text: string }
	| { type: 'tool'; entry: ToolEntry }
	| { type: 'sources'; paths: string[] }
	| { type: 'done'; error?: string };

export const MAX_COMMAND_OUTPUT = 16000;
export const MAX_QUESTION = 4000;
// Research budgets reserve a final answer turn; they must never discard the answer.
export const MAX_TOOL_CALLS = 32;
export const COACH_IDLE_TIMEOUT_MS = 120000;

export function scopeOf(snapshot: Snapshot | null): string {
	return snapshot
		? `${snapshot.sessionId}:${snapshot.observation.context?.playerId ?? 'none'}`
		: 'none';
}

export function validateConnection(connection: Connection): void {
	if (!connection.model.trim() || connection.model.length > 200)
		throw new Error('Enter a model ID.');
	if (connection.provider === 'openrouter' && !connection.apiKey.trim())
		throw new Error('Enter your API key.');
	const url = new URL(
		connection.provider === 'openrouter'
			? 'https://openrouter.ai/api/v1'
			: connection.baseUrl,
	);
	if (
		url.username ||
		url.password ||
		url.search ||
		url.hash ||
		(url.protocol !== 'https:' &&
			!(
				url.protocol === 'http:' &&
				['localhost', '127.0.0.1', '[::1]'].includes(url.hostname)
			))
	) {
		throw new Error('Use an HTTPS API base URL, or HTTP on localhost.');
	}
}
