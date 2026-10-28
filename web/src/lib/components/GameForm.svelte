<script lang="ts">
	import * as Field from '$lib/components/ui/field';
	import { Input } from '$lib/components/ui/input';
	import { NativeSelect } from '$lib/components/ui/native-select';
	import { cleanLabel } from '$lib/encoding';
	import type { UIRequest } from '$lib/types';
	let {
		request,
		values = $bindable(),
		submit,
	}: {
		request: Extract<UIRequest, { kind: 'form' }>;
		values: string[];
		submit: (button: number) => void;
	} = $props();
	const nameField = $derived(
		request.fields.find((field) => field.maxLength !== null),
	);
</script>

<form
	id="answer-form"
	data-game-form
	onsubmit={(event) => {
		event.preventDefault();
		submit(1);
	}}
>
	{#if request.prompt.length}<p
			class="mb-4 whitespace-pre-wrap text-sm text-muted-foreground"
		>
			{request.prompt.join('\n')}
		</p>{/if}
	{#if nameField}<p
			id="game-name-hint"
			class="mb-3 text-sm text-muted-foreground"
		>
			Player names: {nameField.minLength}–{nameField.maxLength} characters.
		</p>{/if}
	<Field.Group class="gap-3">
		{#each request.fields as field, i (field.id)}
			<Field.Field
				class="grid grid-cols-[minmax(0,1fr)_minmax(7rem,1fr)] items-center gap-x-4 gap-y-1"
			>
				<Field.Label for={'game-field-' + field.id}
					>{cleanLabel(field.label)}</Field.Label
				>
				{#if field.choices.length}
					<NativeSelect
						id={'game-field-' + field.id}
						bind:value={values[i]}
						onchange={() => {
							if (field.refreshOnChange) submit(0);
						}}
					>
						{#each field.choices as choice}<option value={choice.value}
								>{cleanLabel(choice.label)}</option
							>{/each}
					</NativeSelect>
				{:else}
					<Input
						id={'game-field-' + field.id}
						bind:value={values[i]}
						type={field.min !== null ? 'number' : 'text'}
						inputmode={field.numeric ? 'decimal' : 'text'}
						class={field.numeric ? 'wsr-money' : undefined}
						min={field.min ?? undefined}
						max={field.max ?? undefined}
						minlength={field.minLength ?? undefined}
						maxlength={field.maxLength ?? undefined}
						required={field.min !== null || field.minLength !== null}
						aria-describedby={field.maxLength
							? 'game-name-hint'
							: field.hint
								? 'game-hint-' + field.id
								: undefined}
					/>
				{/if}
				{#if field.hint}<Field.Description
						id={'game-hint-' + field.id}
						class="col-span-2"
					>
						{field.hint}
					</Field.Description>{/if}
			</Field.Field>
		{/each}
	</Field.Group>
</form>
