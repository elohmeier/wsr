import { validateConnection } from './types';
import type { Connection } from './types';

export const CONNECTION_STORAGE_KEY = 'wsr-coach-connection-v1';
type ConnectionStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
export interface ConnectionProfile extends Connection {
	rememberApiKey: boolean;
}

export function loadConnection(
	storage: ConnectionStorage,
): ConnectionProfile | null {
	// Storage access failures reach the UI; malformed or obsolete records are ignored.
	const raw = storage.getItem(CONNECTION_STORAGE_KEY);
	if (!raw) return null;
	try {
		const value = JSON.parse(raw);
		if (
			!value ||
			value.version !== 1 ||
			!['openrouter', 'compatible'].includes(value.provider) ||
			typeof value.model !== 'string' ||
			typeof value.baseUrl !== 'string' ||
			typeof value.rememberApiKey !== 'boolean' ||
			(value.rememberApiKey && typeof value.apiKey !== 'string')
		)
			return null;
		const profile: ConnectionProfile = {
			provider: value.provider,
			model: value.model,
			baseUrl: value.baseUrl,
			apiKey: value.rememberApiKey ? value.apiKey : '',
			rememberApiKey: value.rememberApiKey,
		};
		validateConnection(profile, { requireApiKey: false });
		return profile;
	} catch {
		return null;
	}
}

export function saveConnection(
	storage: ConnectionStorage,
	connection: Connection,
	rememberApiKey: boolean,
): void {
	validateConnection(connection);
	// Explicit fields keep conversations, snapshots and game files out of settings.
	storage.setItem(
		CONNECTION_STORAGE_KEY,
		JSON.stringify({
			version: 1,
			provider: connection.provider,
			model: connection.model.trim(),
			baseUrl: connection.baseUrl.trim(),
			rememberApiKey,
			...(rememberApiKey ? { apiKey: connection.apiKey } : {}),
		}),
	);
}

export function forgetConnection(storage: ConnectionStorage): void {
	storage.removeItem(CONNECTION_STORAGE_KEY);
}
