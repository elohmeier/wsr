<script lang="ts">
	import type { Source } from '$lib/coach/types';
	let {
		text,
		sources,
		allowed,
		onSource,
	}: {
		text: string;
		sources: Source[];
		allowed: string[];
		onSource: (source: Source, line?: number) => void;
	} = $props();
	const parts = $derived(
		text.split(/(\[[^\]\n]+\]\(\/docs\/[^)\s]+\)|\*\*[^*\n]+\*\*|`[^`\n]+`)/g),
	);
</script>

<div class="coach-prose">
	{#each parts as part, index (index)}{@const citation = part.match(
			/^\[([^\]]+)\]\((\/docs\/[^#)]+)(?:#L(\d+)(?:-L?\d+)?)?\)$/,
		)}{@const source = citation
			? sources.find((s) => s.path === citation[2] && allowed.includes(s.path))
			: undefined}{#if source && citation}<button
				type="button"
				class="coach-citation"
				onclick={() => onSource(source, Number(citation[3]) || 1)}
				>{citation[1]}</button
			>{:else if part.startsWith('**') && part.endsWith('**')}<strong
				>{part.slice(2, -2)}</strong
			>{:else if part.startsWith('`') && part.endsWith('`')}<code
				>{part.slice(1, -1)}</code
			>{:else}{part}{/if}{/each}
</div>
