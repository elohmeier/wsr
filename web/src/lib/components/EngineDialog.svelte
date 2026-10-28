<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Field from '$lib/components/ui/field';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { answer, openDocs } from '$lib/game';
	import { cleanLabel, cp1252, gridPacket } from '$lib/encoding';
	import type { UIRequest, GridRow } from '$lib/types';
	import { untrack } from 'svelte';
	import { cn } from '$lib/utils';
	import Ownership from './Ownership.svelte';
	import OptionGrid from './OptionGrid.svelte';
	let { request: r }: { request: UIRequest } = $props();
	let value = $state(untrack(() => (r.kind === 'inputbox' ? r.value : '')));
	const acknowledgement = $derived(
		r.kind === 'choose' && r.options.length === 1 && cleanLabel(r.options[0]) === 'OK',
	);
	// Legacy input requests have no field type; keep their initial numeric/amount
	// presentation stable while editing, including when the field is cleared.
	const numericInput = $derived(
		r.kind === 'inputbox' &&
			(/^[\s\p{Sc}()+\-]*\d[\d\s.,%()+\-]*$/u.test(r.value) ||
				/\b(amount|how much|price)\b/i.test(r.prompt)),
	);
	let rows = $state<GridRow[]>(
		untrack(() =>
			r.kind === 'optionStation' ? r.rows.map((row) => ({ ...row })) : [],
		),
	);
	const choices: Record<number, [string, number][]> = {
		0: [['OK', 1]],
		1: [
			['OK', 1],
			['Cancel', 2],
		],
		2: [
			['Abort', 3],
			['Retry', 4],
			['Ignore', 5],
		],
		3: [
			['Yes', 6],
			['No', 7],
			['Cancel', 2],
		],
		4: [
			['Yes', 6],
			['No', 7],
		],
		5: [
			['Retry', 4],
			['Cancel', 2],
		],
	};
	function cancel() {
		if (acknowledgement) answer(0);
		else if (r.kind === 'msgbox') {
			const type = r.style & 15;
			// Win32 offers no close/cancel action for these button sets.
			if (type === 2 || type === 4) return;
			answer(type === 4 ? 7 : type ? 2 : 1);
		} else if (r.kind === 'optionStation') answer(-1, gridPacket(rows));
		else
			answer(
				r.kind === 'inputbox' || r.kind === 'chart'
					? 0
					: r.kind === 'report'
						? 2
						: -1,
			);
	}
</script>

<Dialog.Root
	open
	onOpenChange={(open) => {
		if (!open) cancel();
	}}
>
	<Dialog.Content
		id="dlg"
		data-kind={r.kind}
		data-acknowledgement={acknowledgement || undefined}
		class="wsr-dialog flex max-h-[90dvh] flex-col sm:max-w-5xl"
		showCloseButton={false}
		onEscapeKeydown={(event) => {
			if (r.kind === 'msgbox' && [2, 4].includes(r.style & 15))
				event.preventDefault();
		}}
		onInteractOutside={(event) => {
			if (r.kind === 'msgbox') event.preventDefault();
		}}
		onOpenAutoFocus={(e) => {
			if (r.kind === 'msgbox') {
				e.preventDefault();
				const buttons = choices[r.style & 15] || choices[0];
				const value =
					buttons[Math.min((r.style >> 8) & 3, buttons.length - 1)][1];
				document
					.querySelector<HTMLButtonElement>(`[data-msgbox-value="${value}"]`)
					?.focus();
			} else if (r.kind === 'inputbox') {
				e.preventDefault();
				const input = document.getElementById(
					'dlg-input',
				) as HTMLInputElement | null;
				input?.focus();
				input?.select();
			}
		}}
		onkeydown={(e) => {
			if (r.kind === 'msgbox' && e.key === 'Enter') {
				e.preventDefault();
				const buttons = choices[r.style & 15] || choices[0];
				const selected = document.activeElement?.closest<HTMLButtonElement>(
					'[data-msgbox-value]',
				);
				answer(
					selected
						? Number(selected.dataset.msgboxValue)
						: buttons[Math.min((r.style >> 8) & 3, buttons.length - 1)][1],
				);
			}
		}}
	>
		<Dialog.Header class="shrink-0"
			><Dialog.Title id="dlg-title"
				>{r.title || 'Wall Street Raider'}</Dialog.Title
			><Dialog.Description class="sr-only"
				>{r.kind === 'optionStation'
					? 'Enter and calculate up to eight option orders.'
					: 'Game dialog. Choose an action to continue.'}</Dialog.Description
			></Dialog.Header
		>
		<div id="dlg-body" class="flex min-h-0 flex-col gap-4 overflow-auto">
			{#if r.kind === 'msgbox'}<p class="select-text whitespace-pre-wrap">
					{r.text}
				</p>
			{:else if r.kind === 'inputbox'}
				<form
					id="answer-form"
					onsubmit={(e) => {
						e.preventDefault();
						answer(1, cp1252(value));
					}}
				>
					<Field.Group
						><Field.Field
							><Field.Label for="dlg-input" class="whitespace-pre-wrap"
								>{r.prompt}</Field.Label
							><Input
								id="dlg-input"
								class={numericInput ? 'wsr-money' : undefined}
								bind:value
								aria-label={r.title || 'Your answer'}
							/></Field.Field
						></Field.Group
					>
				</form>
			{:else if r.kind === 'choose'}<div class="whitespace-pre-wrap">
					{r.prompt.join('\n')}
				</div>
				{#if !acknowledgement}<div class="grid gap-2 sm:grid-cols-2">
					{#each r.options as option, i}<Button
							data-choice={i}
							variant="outline"
							class="h-auto min-h-9 justify-start whitespace-pre-wrap py-2"
							onclick={() => answer(i)}>{cleanLabel(option)}</Button
						>{/each}
				</div>{/if}
			{:else if r.kind === 'report'}<pre
					class="report select-text overflow-auto bg-muted p-4 font-mono text-xs leading-relaxed">{#each r.lines as line}{@const match =
							line.match(/@([CI])\s*(\d+)\s*$/)}{#if match}<a
								href={'#' + match[0]}
								onclick={(e) => {
									e.preventDefault();
									answer(3, cp1252(match[0]));
								}}>{line.slice(0, match.index).trimEnd()}</a
							>{:else}{line}{/if}{'\n'}{/each}</pre>
			{:else if r.kind === 'chart'}{#await import('./HistoryChart.svelte')}<p
						role="status"
					>
						Loading chart…
					</p>{:then module}<module.default request={r} />{/await}
			{:else if r.kind === 'ownership'}<Ownership request={r} select={answer} />
			{:else if r.kind === 'optionStation'}<OptionGrid
					request={r}
					bind:rows
				/>{/if}
		</div>
		<Dialog.Footer
			id="dlg-buttons"
			class={cn(
				'shrink-0 flex-wrap gap-2',
				r.kind === 'optionStation' && 'flex-row',
			)}
		>
			{#if r.kind === 'msgbox'}{#each choices[r.style & 15] || choices[0] as [label, value]}<Button
						data-msgbox-value={value}
						variant={value === 1 || value === 6 ? 'default' : 'outline'}
						onclick={() => answer(value)}>{label}</Button
					>{/each}
			{:else if r.kind === 'inputbox'}<Button type="submit" form="answer-form"
					>OK</Button
				><Button variant="outline" onclick={() => answer(0)}>Cancel</Button>
			{:else if r.kind === 'choose' && acknowledgement}<Button
					onclick={() => answer(0)}>OK</Button
				>
			{:else if r.kind === 'choose'}<Button
					variant="outline"
					onclick={() => answer(-1)}>Cancel</Button
				>
			{:else if r.kind === 'report'}<Button
					variant="outline"
					onclick={() => window.print()}>Print</Button
				>{#if r.stockChart}<Button
						data-report-chart
						variant="outline"
						onclick={() => answer(4)}>Stock chart</Button
					>{/if}
				{#if r.analystRating !== null}<Button
						data-report-rating
						variant="outline"
						disabled={!r.ratingTrade}
						onclick={() => answer(5)}
					>
						{['Not traded', 'Strong buy', 'Buy', 'Hold', 'Sell', 'Strong sell'][
							r.analystRating
						] || 'Not traded'}
					</Button>{/if}
				{#if r.yesNo}<Button onclick={() => answer(0)}>Yes</Button><Button
						variant="outline"
						onclick={() => answer(1)}>No</Button
					>{:else}<Button onclick={() => answer(0)}>OK</Button>{/if}
			{:else if r.kind === 'ownership'}<Button
					variant="outline"
					onclick={() => answer(0)}>Close</Button
				>
			{:else if r.kind === 'chart'}<Button
					variant="outline"
					onclick={() => window.print()}>Print</Button
				>{#if r.tradeLabel}<Button onclick={() => answer(1)}
						>{cleanLabel(r.tradeLabel)}</Button
					>{/if}<Button variant="outline" onclick={() => answer(0)}
					>Close</Button
				>
			{:else if r.kind === 'optionStation'}{#each r.buttons as button (button.id)}<Button
						variant={button.id === 1374 ? 'default' : 'outline'}
						onclick={() =>
							button.id === 1376
								? openDocs('help', 'advanced options')
								: answer(button.id, gridPacket(rows))}>{button.label}</Button
					>{/each}{/if}
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
