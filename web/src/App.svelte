<script lang="ts">
	import { onMount } from 'svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { Toaster } from '$lib/components/ui/sonner';
	import { game, start, openDocs, retryStorage } from '$lib/game';
	import { assetUrl } from '../docs.mjs';
	import FilesPanel from '$lib/components/FilesPanel.svelte';
	import DeskMenu from '$lib/components/DeskMenu.svelte';
	import Desk from '$lib/components/Desk.svelte';
	import EngineDialog from '$lib/components/EngineDialog.svelte';
	import Documentation from '$lib/components/Documentation.svelte';
	import Customizer from '$lib/components/Customizer.svelte';
	import ResearchLists from '$lib/components/ResearchLists.svelte';
	let filesOpen = $state(true);
	$effect(() => {
		if (!$game.ready && $game.summary) filesOpen = true;
	});
	const prepared = $derived(
		['WSR101.DAT', 'MSCNEWS.DAT', 'SCORES.WSR', 'CORPNAME.DAT', 'ENGINE-CONTENT.WSR'].every((n) =>
			$game.files.some((f) => f.name === n),
		),
	);
	const banner = $derived(
		$game.files.some((f) => f.name === 'WSR-BANNER.BMP')
			? assetUrl('WSR-BANNER.BMP')
			: undefined,
	);
	onMount(() => {
		void start();
	});
</script>

<svelte:window
	onkeydown={(event) => {
		if (event.key === 'F1') {
			event.preventDefault();
			openDocs(
				'help',
				$game.ui?.kind === 'optionStation' ? 'advanced options' : '',
			);
		}
	}}
/>
<div class="app-shell">
	<header>
		<DeskMenu onFiles={() => (filesOpen = true)} />
		<div class="wsr-brand">
			{#if banner}<img
					src={banner}
					alt="Wall Street Raider"
					width="998"
					height="105"
				/>
				<h1 class="sr-only">Wall Street Raider</h1>{:else}<h1>
					Wall <span>$</span>treet Raider
				</h1>
				<p>The corporate financial simulation</p>{/if}
		</div>
	</header>
	<main>
		{#if $game.storageError}<Alert.Root
				id="storage-warning"
				variant="destructive"
			>
				<Alert.Title
					>{$game.storageBlocked
						? 'Game open in another tab'
						: 'Browser saves need attention'}</Alert.Title
				>
				<Alert.Description>
					<p>{$game.storageError}</p>
					{#if $game.unsavedFiles}<p>
							{$game.unsavedFiles} file changes are waiting to be stored. Downloads
							include the latest saved bytes.
						</p>{/if}
					{#if $game.storageBlocked}<Button
							variant="outline"
							onclick={() => location.reload()}>Reload</Button
						>
					{:else}<Button
							id="retry-storage"
							variant="outline"
							disabled={$game.busy || !$game.ready || !$game.canRetryStorage}
							onclick={retryStorage}>Retry storage</Button
						>{/if}
				</Alert.Description>
			</Alert.Root>{/if}
		{#if !$game.summary}<section id="welcome" class="wsr-welcome">
				<h2>Your trading desk</h2>
				<p>
					Open wsr985.exe from your copy of Wall Street Raider, then choose New
					game. Your game and saves stay in this browser.
				</p>
				<div class="wsr-welcome-actions">
					<Button
						id="new-game"
						disabled={!$game.ready || $game.busy || !prepared}
						onclick={() => game.action(1)}>New game</Button
					>
				</div>
			</section>{/if}
		<div hidden={!!$game.summary && !filesOpen}>
			<FilesPanel bind:open={filesOpen} />
		</div>
		<Alert.Root id="error" variant="destructive" hidden={!$game.error}
			><Alert.Title>Unable to complete the action</Alert.Title
			><Alert.Description>{$game.error}</Alert.Description></Alert.Root
		>
		{#if $game.summary}<Desk />{/if}
		{#if $game.requests.some((r) => r.kind === 'exit' || (r.kind === 'url' && /^https?:\/\//i.test(r.value || '')))}<section
				id="host-requests"
				class="wsr-panel"
			>
				<h2>Game links</h2>
				<div class="wsr-panel-content">
					{#each $game.requests as request}{#if request.kind === 'exit'}<p>
								Game closed. Open a saved game or start a new one to continue.
							</p>{:else if request.kind === 'url' && /^https?:\/\//i.test(request.value || '')}<a
								href={request.value}
								target="_blank"
								rel="noopener noreferrer">{request.value}</a
							>{/if}{/each}
				</div>
			</section>{/if}
	</main>
</div>
{#if $game.ui}{#key $game.ui.token}<EngineDialog
			request={$game.ui}
		/>{/key}{/if}
{#if $game.panel?.kind === 'docs'}<Documentation
		initialMode={$game.panel.mode}
		initialQuery={$game.panel.query}
	/>{:else if $game.panel?.kind === 'customizer'}<Customizer
	/>{:else if $game.panel?.kind === 'players' || $game.panel?.kind === 'companies'}<ResearchLists
		kind={$game.panel.kind}
	/>{/if}
{#if $game.coachOpened}{#await import('$lib/components/Coach.svelte') then module}<module.default
		/>{/await}{/if}
<div id="notices">
	<Toaster theme="dark" position="bottom-right" closeButton />
</div>
