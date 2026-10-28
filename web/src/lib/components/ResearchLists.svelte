<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Table from '$lib/components/ui/table';
	import { Button } from '$lib/components/ui/button';
	import { game, closePanel } from '$lib/game';
	let { kind }: { kind: 'players' | 'companies' } = $props();
	const s = $derived($game.summary!);
	const money = (n: number | null) =>
		n == null
			? '—'
			: s.currencySymbol.trim() +
				n.toLocaleString(undefined, { maximumFractionDigits: 2 }) +
				' ' +
				s.moneyUnit;
	function report(kind: number, entity = 0) {
		closePanel();
		game.report(kind, entity);
	}
	function company(id: number) {
		closePanel();
		game.action(6, id);
	}
</script>

<Dialog.Root
	open
	onOpenChange={(open) => {
		if (!open) closePanel();
	}}
	><Dialog.Content
		class="wsr-dialog wsr-list-dialog flex max-h-[90dvh] flex-col sm:max-w-5xl"
		showCloseButton={false}
		><Dialog.Header class="shrink-0"
			><Dialog.Title
				>{kind === 'players'
					? 'Players & standings'
					: 'Largest companies'}</Dialog.Title
			><Dialog.Description
				>{kind === 'players'
					? 'Current players and their financial reports.'
					: 'Companies ranked by market capitalization.'}</Dialog.Description
			></Dialog.Header
		>
		<div class="min-h-0 overflow-auto">
			{#if kind === 'players'}<Table.Root
					><Table.Header
						><Table.Row
							><Table.Head>Player</Table.Head><Table.Head class="text-right"
								>Cash</Table.Head
							><Table.Head>Reports</Table.Head></Table.Row
						></Table.Header
					><Table.Body id="players"
						>{#each s.players as p (p.id)}<Table.Row
								><Table.Cell
									>{p.name}
									{p.computer ? '· computer' : ''}
									{p.id === s.currentPlayer ? '← to move' : ''}</Table.Cell
								><Table.Cell class="wsr-money text-right"
									>{money(p.cash)}</Table.Cell
								><Table.Cell
									>{#if p.id === s.currentPlayer}<Button
											variant="outline"
											onclick={() => report(8)}>Financial profile</Button
										>{/if}<Button
										variant="outline"
										onclick={() => report(7, p.id)}>Portfolio</Button
									></Table.Cell
								></Table.Row
							>{/each}</Table.Body
					></Table.Root
				>
			{:else}<Table.Root
					><Table.Header
						><Table.Row
							>{#each ['Symbol', 'Company', 'Industry', 'Market cap', 'Price'] as label}<Table.Head
									class={label === 'Market cap' || label === 'Price'
										? 'text-right'
										: ''}>{label}</Table.Head
								>{/each}</Table.Row
						></Table.Header
					><Table.Body id="companies"
						>{#each s.topCorps as c (c.id)}<Table.Row
								><Table.Cell
									><Button variant="link" onclick={() => company(c.id)}
										>{c.symbol}</Button
									></Table.Cell
								><Table.Cell>{c.name}</Table.Cell><Table.Cell
									>{c.industry}</Table.Cell
								><Table.Cell class="wsr-money text-right"
									>{money(c.marketCapM)}</Table.Cell
								><Table.Cell class="wsr-money text-right"
									>{c.price == null
										? '—'
										: s.currencySymbol.trim() + c.price.toFixed(2)}</Table.Cell
								></Table.Row
							>{/each}</Table.Body
					></Table.Root
				>{/if}
		</div>
		<Dialog.Footer class="shrink-0 flex-wrap"
			>{#if kind === 'players'}<Button
					variant="outline"
					onclick={() => report(11)}>Standings</Button
				>{:else}<Button variant="outline" onclick={() => report(9, 4)}
					>Top 100 report</Button
				>{/if}<Button variant="outline" onclick={closePanel}>Close</Button
			></Dialog.Footer
		></Dialog.Content
	></Dialog.Root
>
