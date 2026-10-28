<script lang="ts">
	import { game } from '$lib/game';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import { cleanLabel } from '$lib/encoding';
	import type { UIRequest } from '$lib/types';
	let {
		request,
		values = $bindable(),
		submit,
	}: {
		request: Extract<UIRequest, { kind: 'databaseSearch' }>;
		values: string[];
		submit: (button: number, checked?: string) => void;
	} = $props();
	const fields = $derived(request.criteria.flatMap((c) => c.fields));
	$effect(() => {
		const changed = fields.find((field, i) => field.value !== values[i]);
		if ($game.panel || !changed) return;
		const timer = setTimeout(() => submit(changed.id), 500);
		return () => clearTimeout(timer);
	});
</script>

<form
	id="database-form"
	data-database-search
	onsubmit={(event) => {
		event.preventDefault();
		submit(387);
	}}
>
	<p role="status" class="mb-3 font-medium" data-search-count>
		{request.count}
	</p>
	<p class="mb-4 text-sm text-muted-foreground">{request.prompt}</p>
	<div class="grid gap-2 lg:grid-cols-2">
		{#each request.criteria as criterion (criterion.id)}
			<div class="flex flex-col gap-2 rounded border border-input p-3">
				<label
					class="flex items-start gap-3 leading-snug"
					for={'database-control-' + criterion.id}
				>
					<input
						id={'database-control-' + criterion.id}
						type="checkbox"
						checked={criterion.checked}
						class="mt-0.5 size-4 shrink-0 accent-primary"
						onchange={(event) =>
							submit(criterion.id, event.currentTarget.checked ? '1' : '0')}
					/>
					{cleanLabel(criterion.label)}
				</label>
				{#if criterion.fields.length}<div class="flex gap-2 pl-7">
						{#each criterion.fields as field (field.id)}
							{@const i = fields.findIndex((f) => f.id === field.id)}
							<label
								class="min-w-0 flex-1"
								for={'database-control-' + field.id}
							>
								<span class="sr-only">{field.label}</span>
								<Input
									id={'database-control-' + field.id}
									bind:value={values[i]}
									inputmode="decimal"
									class="wsr-money"
								/>
							</label>
						{/each}
					</div>{/if}
				{#if criterion.change}<div
						class="flex items-center justify-between gap-2 pl-7"
					>
						<span data-search-rating={criterion.change.id}
							>{criterion.change.value}</span
						>
						<Button
							id={'database-control-' + criterion.change.id}
							type="button"
							variant="outline"
							aria-label={criterion.change.label}
							onclick={() => submit(criterion.change!.id)}>Change</Button
						>
					</div>{/if}
			</div>
		{/each}
	</div>
</form>
