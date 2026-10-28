<script lang="ts">
	import * as Table from '$lib/components/ui/table';
	import * as NativeSelect from '$lib/components/ui/native-select';
	import { Input } from '$lib/components/ui/input';
	import type { GridRow, UIRequest } from '$lib/types';
	let {
		request,
		rows = $bindable(),
	}: {
		request: Extract<UIRequest, { kind: 'optionStation' }>;
		rows: GridRow[];
	} = $props();
</script>

<p>{request.stock}</p>
<p class="text-sm text-muted-foreground" data-option-bounds>
	{request.strikeHeader} · {request.expiryHeader}
</p>
<Table.Root class="option-grid min-w-[760px]">
	<Table.Header
		><Table.Row
			>{#each ['Line', 'Type', 'Buy / sell', 'Strike price', 'Expiry (month/year)', 'Size (%)', 'Option price'] as label}<Table.Head
					>{label}</Table.Head
				>{/each}</Table.Row
		></Table.Header
	>
	<Table.Body>
		{#each rows as row, k (k)}<Table.Row>
				<Table.Cell>{k + 1}</Table.Cell><Table.Cell
					>{k < 4 ? 'Put' : 'Call'}</Table.Cell
				>
				<Table.Cell
					><NativeSelect.Root
						data-grid
						aria-label={'Line ' + (k + 1) + ' buy or sell'}
						bind:value={row.side}
						><NativeSelect.Option value="">—</NativeSelect.Option
						><NativeSelect.Option value="B">Buy</NativeSelect.Option
						><NativeSelect.Option value="S">Sell short</NativeSelect.Option
						></NativeSelect.Root
					></Table.Cell
				>
				<Table.Cell
					><Input
						data-grid
						aria-label={'Line ' + (k + 1) + ' strike'}
						class="wsr-money text-right"
						bind:value={row.strike}
						inputmode="decimal"
						maxlength={64}
					/></Table.Cell
				>
				<Table.Cell
					><Input
						data-grid
						aria-label={'Line ' + (k + 1) + ' expiry'}
						bind:value={row.expiry}
						maxlength={64}
					/></Table.Cell
				>
				<Table.Cell
					><Input
						data-grid
						aria-label={'Line ' + (k + 1) + ' quantity'}
						bind:value={row.quantity}
						inputmode="decimal"
						maxlength={64}
					/></Table.Cell
				>
				<Table.Cell class="wsr-money text-right">{row.price || '—'}</Table.Cell>
			</Table.Row>{/each}
	</Table.Body>
</Table.Root>
<p role="status">{request.status}</p>
