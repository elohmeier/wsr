<script lang="ts">
	import type { UIRequest } from '$lib/types';
	let {
		request,
		select,
	}: {
		request: Extract<UIRequest, { kind: 'ownership' }>;
		select: (id: number) => void;
	} = $props();
</script>

<svg
	class="ownership w-full min-h-60"
	viewBox={'0 0 ' + request.width + ' ' + request.height}
	aria-label={request.title}
	role="img"
>
	{#each request.controls as control, i (i)}
		{#if control.kind === 'line'}<rect
				x={control.x}
				y={control.y}
				width={Math.max(0.5, control.w)}
				height={Math.max(0.5, control.h)}
				fill="currentColor"
			/>
		{:else if control.kind !== 'button'}
			{#snippet box()}
				{#if control.clickable || control.id === 454}<rect
						x={control.x}
						y={control.y}
						width={control.w}
						height={control.h}
						fill={control.id === 454 ? 'var(--primary)' : 'var(--muted)'}
						stroke="var(--border)"
						stroke-width=".5"
					/>{/if}
				{#each control.text.split(/\r\n?|\n/) as line, k}<text
						x={control.x + control.w / 2}
						y={control.y + 8 + k * 7}
						text-anchor="middle"
						fill={control.id === 454
							? 'var(--primary-foreground)'
							: 'var(--foreground)'}>{line}</text
					>{/each}
			{/snippet}
			{#if control.clickable}<a
					href={'#company-' + control.id}
					data-diagram={control.id}
					aria-label={control.text}
					onclick={(e) => {
						e.preventDefault();
						select(control.id);
					}}>{@render box()}</a
				>{:else}{@render box()}{/if}
		{/if}
	{/each}
</svg>

<style>
	.ownership {
		font: 7px var(--font-sans);
	}
	a:hover rect,
	a:focus rect {
		stroke: var(--ring);
		stroke-width: 1.5;
	}
</style>
