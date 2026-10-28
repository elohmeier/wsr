<script lang="ts">
	import * as Menubar from '$lib/components/ui/menubar';
	import { Button } from '$lib/components/ui/button';
	import {
		game,
		openDocs,
		openCustomizer,
		openList,
		openCoach,
	} from '$lib/game';
	import { cleanLabel } from '$lib/encoding';
	let { onFiles }: { onFiles: () => void } = $props();
	const groups = $derived(
		$game.menus.length
			? [...Map.groupBy($game.menus, (m) => m.group)]
			: ([
					['&File', []],
					['&Game Options', []],
					['&Settings', []],
					['&Help', []],
				] as [string, import('$lib/types').MenuItem[]][]),
	);
	function command(id: number) {
		if (id === 610) {
			onFiles();
			return;
		}
		if (id === 655) {
			openCustomizer();
			return;
		}
		if (id === 678) {
			openDocs();
			return;
		}
		if (id === 656 || id === 658) {
			openDocs('manual');
			return;
		}
		game.action(id === 605 ? 1 : 3, id === 605 ? 0 : id);
	}
</script>

<div class="wsr-menu-row">
	<Menubar.Root id="menus" aria-label="Game menus" class="wsr-menubar">
		{#each groups as [group, items] (group)}
			<Menubar.Menu
				><Menubar.Trigger disabled={!$game.ready || $game.busy}
					>{cleanLabel(group)}</Menubar.Trigger
				>
				<Menubar.Content class="wsr-menu-popup"
					><Menubar.Group>
						{#if group === '&File'}
							<Menubar.Item data-files-menu onSelect={onFiles}
								>Game files & saved games…</Menubar.Item
							>
							<Menubar.Separator />
						{/if}
						{#each items as item, i (item.id)}
							{#if item.label === '-'}<Menubar.Separator />{:else}<Menubar.Item
									disabled={item.disabled && item.id !== 610}
									onSelect={() => command(item.id)}
									>{cleanLabel(item.label)}</Menubar.Item
								>{/if}
						{/each}
						{#if !items.length && group === '&File'}<Menubar.Item
								disabled={!$game.files.some((f) => f.name === 'WSR101.DAT')}
								onSelect={() => game.action(1)}>New game</Menubar.Item
							>{/if}
						{#if group === '&Help' && !items.length}<Menubar.Item
								onSelect={() => openDocs()}>Help & manual</Menubar.Item
							>{/if}
						{#if group === '&Game Options'}<Menubar.Separator /><Menubar.Item
								disabled={!$game.summary}
								onSelect={() => openList('players')}
								>Players & standings</Menubar.Item
							><Menubar.Item
								disabled={!$game.summary}
								onSelect={() => openList('companies')}
								>Largest companies</Menubar.Item
							>{/if}
					</Menubar.Group></Menubar.Content
				>
			</Menubar.Menu>
		{/each}
	</Menubar.Root>
	<Button
		id="save-game"
		variant="ghost"
		size="sm"
		disabled={!$game.ready ||
			!$game.summary?.gameInProgress ||
			$game.summary?.gameOver ||
			$game.busy}
		onclick={() => game.action(3, 615)}>Save game</Button
	>
	<Button
		id="customizer"
		variant="ghost"
		size="sm"
		disabled={!$game.ready ||
			$game.busy ||
			!$game.files.some((f) => f.name === 'CORPNAME.DAT')}
		onclick={openCustomizer}>Company Customizer</Button
	>
	<Button id="help" variant="ghost" size="sm" onclick={() => openDocs()}
		>Help & manual</Button
	>
	<Button id="open-coach" variant="ghost" size="sm" onclick={() => openCoach()}
		>Learning coach</Button
	>
	<span id="progress" role="status">{$game.progress}</span><span
		id="status"
		role="status">{$game.status}</span
	>
</div>
