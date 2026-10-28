<script lang="ts">
	import { tick } from 'svelte';
	import { Textarea } from '$lib/components/ui/textarea';
	import type { Source } from '$lib/coach/types';
	let { source, line }: { source: Source; line: number } = $props();
	let viewport = $state<HTMLTextAreaElement | null>(null);
	const lines = $derived([source.title, '', ...source.text.split('\n')]);
	const selected = $derived(
		Math.max(1, Math.min(lines.length, Math.floor(line) || 1)),
	);
	const text = $derived(
		lines.map((value, i) => `${i + 1}: ${value}`).join('\n'),
	);
	$effect(() => {
		const targetLine = selected;
		const content = text;
		void tick().then(() => {
			if (!viewport) return;
			const start = content
				.split('\n')
				.slice(0, targetLine - 1)
				.reduce((n, value) => n + value.length + 1, 0);
			viewport.setSelectionRange(
				start,
				content.indexOf('\n', start) < 0
					? content.length
					: content.indexOf('\n', start),
			);
			viewport.scrollTop = Math.max(
				0,
				(targetLine - 1) * parseFloat(getComputedStyle(viewport).lineHeight) -
					viewport.clientHeight / 3,
			);
		});
	});
</script>

<Textarea
	class="coach-source-text"
	bind:ref={viewport}
	value={text}
	readonly
	rows={18}
	aria-label="Source text with line numbers"
/>
