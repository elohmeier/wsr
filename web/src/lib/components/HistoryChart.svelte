<script lang="ts">
	import { LineChart } from 'layerchart';
	import { scaleTime } from 'd3-scale';
	import * as Chart from '$lib/components/ui/chart';
	import * as Empty from '$lib/components/ui/empty';
	import type { UIRequest } from '$lib/types';
	let { request }: { request: Extract<UIRequest, { kind: 'chart' }> } =
		$props();
	const data = $derived(
		request.history.map((row) => ({
			...row,
			date: new Date(request.endYear, request.endMonth - 1 + row.month - 60, 1),
		})),
	);
	const config = {
		close: { label: 'Month-end close', color: 'var(--chart-1)' },
		high: { label: 'High', color: 'var(--chart-1)' },
		low: { label: 'Low', color: 'var(--chart-1)' },
	} satisfies Chart.ChartConfig;
	const percent = $derived(request.entity >= 1601 && request.entity <= 1604);
</script>

{#if data.length}
	<div class="chart" aria-label={request.title + ' monthly history'}>
		<Chart.Container
			{config}
			class="w-full aspect-auto"
			style={`height: ${Math.max(120, request.height - 52)}px`}
		>
			<LineChart
				{data}
				x="date"
				xScale={scaleTime()}
				y={['high', 'close', 'low']}
				yBaseline={undefined}
				yNice
				seriesLayout="overlap"
				padding={{ left: 64, right: 20, top: 12, bottom: 36 }}
				series={[
					{ key: 'high', label: 'High', color: config.high.color },
					{ key: 'close', label: 'Month-end close', color: config.close.color },
					{ key: 'low', label: 'Low', color: config.low.color },
				]}
				props={{
					xAxis: {
						ticks: data
							.filter(
								(_, i) =>
									i % Math.max(1, Math.ceil(data.length / 4)) === 0 ||
									i === data.length - 1,
							)
							.map((d) => d.date),
						format: (v: Date) =>
							v.toLocaleDateString(undefined, {
								month: 'short',
								year: '2-digit',
							}),
					},
					yAxis: {
						format: (v: number) =>
							v.toLocaleString(undefined, {
								notation: 'compact',
								maximumFractionDigits: 2,
							}) + (percent ? '%' : ''),
					},
				}}
			>
				{#snippet marks({ context })}
					<g
						class="hlc-bars"
						stroke="var(--chart-1)"
						stroke-width="1.5"
						fill="none"
					>
						{#each data as row (row.month)}
							{@const x = context.xScale(row.date)}
							<g class="hlc-bar"
								><line
									x1={x}
									x2={x}
									y1={context.yScale(row.high)}
									y2={context.yScale(row.low)}
								/><line
									x1={x}
									x2={x + 5}
									y1={context.yScale(row.close)}
									y2={context.yScale(row.close)}
								/></g
							>
						{/each}
					</g>
				{/snippet}
				{#snippet tooltip()}<Chart.Tooltip
						class="min-w-56"
						labelFormatter={(value) =>
							value instanceof Date
								? value.toLocaleDateString(undefined, {
										month: 'short',
										year: 'numeric',
									})
								: String(value)}
					>
						{#snippet formatter({ value, name })}
							<span class="flex-1 whitespace-nowrap pr-4 text-muted-foreground"
								>{name}</span
							>
							<span class="font-mono font-medium tabular-nums"
								>{Number(value).toLocaleString(undefined, {
									minimumFractionDigits: 2,
									maximumFractionDigits: 2,
								})}{percent ? '%' : ''}</span
							>
						{/snippet}
					</Chart.Tooltip>{/snippet}
			</LineChart>
		</Chart.Container>
		<p class="wsr-caption">
			Monthly high–low range · Right tick marks the month-end close
		</p>
	</div>
{:else}<Empty.Root
		><Empty.Header
			><Empty.Title>No monthly history yet</Empty.Title><Empty.Description
				>Start the ticker to build this series.</Empty.Description
			></Empty.Header
		></Empty.Root
	>{/if}
{#each request.controls.filter((control) => control.id === 521 && control.kind === 'label') as control}<p
		class="wsr-money"
		data-chart-last
	>
		{control.text}
	</p>{/each}
