<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { game } from '$lib/game';
	let {
		control,
		children,
		primary = false,
		id,
	}: {
		control: number;
		children: import('svelte').Snippet;
		primary?: boolean;
		id?: string;
	} = $props();
</script>

<Button
	{id}
	data-control={control}
	variant={primary ? 'default' : 'outline'}
	class="wsr-desk-button"
	disabled={!$game.ready ||
		$game.busy ||
		$game.summary?.disabledControls.includes(control)}
	onclick={() => game.action(2, control)}>{@render children()}</Button
>
