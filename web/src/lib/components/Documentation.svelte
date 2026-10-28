<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Tabs from '$lib/components/ui/tabs';
	import * as Field from '$lib/components/ui/field';
	import * as Empty from '$lib/components/ui/empty';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import { asset, helpTopics, manualBody } from '../../../docs.mjs';
	import { closePanel } from '$lib/game';
	import { untrack } from 'svelte';
	let {
		initialMode,
		initialQuery,
	}: { initialMode: 'help' | 'glossary' | 'manual'; initialQuery: string } =
		$props();
	let mode = $state(untrack(() => initialMode)),
		query = $state(untrack(() => initialQuery));
	const topics = $derived(
		helpTopics(asset('HELP.TXT')).filter((t) =>
			(t.title + '\n' + t.text)
				.toLowerCase()
				.includes(query.trim().toLowerCase()),
		),
	);
	const glossary = $derived(
		new TextDecoder('windows-1252')
			.decode(asset('GLOSSARY.TXT'))
			.split(/\r?\n\s*\r?\n/)
			.filter((p) => p.toLowerCase().includes(query.trim().toLowerCase()))
			.join('\n\n'),
	);
	function manual(node: HTMLElement) {
		const bytes = asset('WSRBOOK.HTM');
		if (!bytes) {
			node.textContent = 'Reopen wsr985.exe to load the strategy manual.';
			return;
		}
		const { out, dispose } = manualBody(bytes);
		out.className = 'manual';
		node.append(out);
		return { destroy: dispose };
	}
</script>

<Dialog.Root
	open
	onOpenChange={(open) => {
		if (!open) closePanel();
	}}
>
	<Dialog.Content
		class="wsr-dialog documentation flex max-h-[90dvh] flex-col sm:max-w-5xl"
		showCloseButton={false}
	>
		<Dialog.Header class="shrink-0"
			><Dialog.Title>Wall Street Raider documentation</Dialog.Title
			><Dialog.Description
				>Help, glossary and strategy manual from your copy of the game.</Dialog.Description
			></Dialog.Header
		>
		<Tabs.Root bind:value={mode} class="flex min-h-0 flex-col gap-4">
			<Tabs.List
				><Tabs.Trigger
					value="help"
					data-mode="help"
					onclick={() => (query = '')}>Help topics</Tabs.Trigger
				><Tabs.Trigger
					value="glossary"
					data-mode="glossary"
					onclick={() => (query = '')}>Glossary</Tabs.Trigger
				><Tabs.Trigger value="manual" data-mode="manual"
					>Strategy manual</Tabs.Trigger
				></Tabs.List
			>
			{#if mode !== 'manual'}<Field.Group
					><Field.Field
						><Field.Label for="docs-search">Search</Field.Label><Input
							id="docs-search"
							type="search"
							data-search
							bind:value={query}
						/></Field.Field
					></Field.Group
				>{/if}
			<Tabs.Content value={mode} class="min-h-0 overflow-auto" data-content>
				{#if mode === 'help'}
					{#if !asset('HELP.TXT')}<p>
							Reopen wsr985.exe to load the help topics.
						</p>
					{:else}<p class="mb-3 text-sm text-muted-foreground">
							{topics.length} matching topics
						</p>
						{#each topics as topic (topic.id)}<details
								class="border-b py-3"
								open={topics.length === 1}
							>
								<summary class="cursor-pointer font-medium"
									>{topic.title}</summary
								>
								<pre
									class="mt-3 select-text whitespace-pre-wrap font-sans leading-relaxed">{topic.text}</pre>
							</details>{/each}{/if}
				{:else if mode === 'glossary'}
					{#if glossary}<pre
							class="select-text whitespace-pre-wrap font-sans leading-relaxed">{glossary}</pre>{:else}<Empty.Root
							><Empty.Header
								><Empty.Title>No matching entries</Empty.Title
								><Empty.Description
									>Try another glossary term.</Empty.Description
								></Empty.Header
							></Empty.Root
						>{/if}
				{:else}<div class="select-text" use:manual></div>{/if}
			</Tabs.Content>
		</Tabs.Root>
		<Dialog.Footer class="shrink-0"
			><Button data-close variant="outline" onclick={closePanel}>Close</Button
			></Dialog.Footer
		>
	</Dialog.Content>
</Dialog.Root>
