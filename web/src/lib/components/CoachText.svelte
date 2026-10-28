<script lang="ts">
	import { Streamdown, lex, parseBlocks } from 'svelte-streamdown';
	import { citationSources, resolveCitation } from '$lib/coach/citations';
	import type { MessageStatus, Source } from '$lib/coach/types';
	import * as Trace from '$lib/components/ui/trace';

	let {
		text,
		sources,
		allowed,
		status = 'complete',
		onSource,
	}: {
		text: string;
		sources: Source[];
		allowed: string[];
		status?: MessageStatus;
		onSource: (source: Source, line?: number) => void;
	} = $props();

	// Parse the source list once generation settles, using the same block parser
	// and citation policy as the renderer. Code examples never count as citations.
	const cited = $derived(
		status === 'streaming'
			? []
			: citationSources(
					parseBlocks(text).flatMap((block) => lex(block)),
					sources,
					allowed,
				),
	);
</script>

<div class="coach-markdown" data-message-status={status}>
	<!-- Keep the block tree mounted at completion; static mode replaces it.
	     Interrupted messages retain repair of their unfinished tail for readability. -->
	<Streamdown
		content={text}
		baseTheme="shadcn"
		parseIncompleteMarkdown={status !== 'complete'}
		renderHtml={false}
		allowedLinkPrefixes={['*']}
		allowedImagePrefixes={[]}
		controls={false}
		animation={{ enabled: false }}
	>
		{#snippet link({ token, children })}
			{@const citation = resolveCitation(token.href, sources, allowed)}
			{#if citation}
				<button
					type="button"
					class="coach-citation"
					onclick={() => onSource(citation.source, citation.line)}
					>{@render children()}</button
				>
			{:else}{@render children()}{/if}
		{/snippet}
		{#snippet image({ token })}{token.text}{/snippet}
		{#snippet math({ token })}{token.raw}{/snippet}
		{#snippet code({ token })}<pre><code>{token.text}</code></pre>{/snippet}
		{#snippet mermaid({ token })}<pre><code>{token.text}</code></pre>{/snippet}
		{#snippet inlineCitation({ token })}{token.raw}{/snippet}
		{#snippet footnoteRef({ token })}{token.raw}{/snippet}
	</Streamdown>
</div>

{#if cited.length}
	<Trace.Collapsible>
		<Trace.CollapsibleTrigger
			><Trace.Line
				><Trace.Title>Sources · {cited.length}</Trace.Title></Trace.Line
			></Trace.CollapsibleTrigger
		>
		<Trace.CollapsibleContent>
			{#each cited as source (source.path)}
				<button
					class="coach-source-link"
					type="button"
					onclick={() => onSource(source)}>{source.title}</button
				>
			{/each}
		</Trace.CollapsibleContent>
	</Trace.Collapsible>
{/if}
