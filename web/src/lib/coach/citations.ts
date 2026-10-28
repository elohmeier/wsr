import type { StreamdownToken } from 'svelte-streamdown';
import type { Source } from './types';

export function resolveCitation(
	href: string,
	sources: Source[],
	allowed: string[],
): { source: Source; line: number } | undefined {
	const match = href.match(
		/^(\/docs\/[^#?\s\\]+)(?:#L([1-9]\d*)(?:-L?([1-9]\d*))?)?$/,
	);
	if (!match || !allowed.includes(match[1])) return;
	const source = sources.find((source) => source.path === match[1]);
	const line = Number(match[2] ?? 1);
	const end = Number(match[3] ?? line);
	if (
		source &&
		Number.isSafeInteger(line) &&
		Number.isSafeInteger(end) &&
		end >= line
	)
		return { source, line };
}

export function citationSources(
	tokens: StreamdownToken[],
	sources: Source[],
	allowed: string[],
): Source[] {
	const paths = new Set<string>();
	function visit(tokens: StreamdownToken[]) {
		for (const token of tokens) {
			if (token.type === 'link') {
				const citation = resolveCitation(token.href, sources, allowed);
				if (citation) paths.add(citation.source.path);
			}
			if ('tokens' in token && Array.isArray(token.tokens))
				visit(token.tokens as StreamdownToken[]);
		}
	}
	visit(tokens);
	return sources.filter((source) => paths.has(source.path));
}
