<script lang="ts">
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Field from '$lib/components/ui/field';
	import * as NativeSelect from '$lib/components/ui/native-select';
	import * as Alert from '$lib/components/ui/alert';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import { game, closePanel, customize } from '$lib/game';
	import { asset } from '../../../docs.mjs';
	import {
		parseCompanies,
		editCompany,
		serializeCompanies,
	} from '../../../companies.mjs';
	interface Company {
		id: number;
		country: number;
		name: string;
		symbol: string;
	}
	const initial = parseCompanies(asset('CORPNAME.DAT'));
	let rows = $state<Company[]>(initial),
		selected = $state('11'),
		query = $state(''),
		name = $state(initial[0].name),
		symbol = $state(initial[0].symbol),
		country = $state(String(initial[0].country)),
		dirty = $state(false),
		message = $state(''),
		invalid = $state(false),
		discard = $state(false);
	const matches = $derived(
		rows.filter((r) =>
			(r.id + ' ' + r.symbol + ' ' + r.name)
				.toUpperCase()
				.includes(query.trim().toUpperCase()),
		),
	);
	function fill() {
		const r = rows.find((r) => r.id === +selected);
		if (r) {
			name = r.name;
			symbol = r.symbol;
			country = String(r.country);
		}
	}
	function apply(event: SubmitEvent) {
		event.preventDefault();
		try {
			rows = editCompany(rows, +selected, { name, symbol, country });
			dirty = true;
			invalid = false;
			message = 'Edit applied. Save for new games to keep your changes.';
		} catch (error) {
			invalid = true;
			message = (error as Error).message;
		}
	}
	async function save() {
		try {
			const stored = await customize(serializeCompanies(rows));
			dirty = false;
			discard = false;
			invalid = false;
			message = stored
				? 'Saved. New games will use this company list.'
				: 'Applied for this session. Close the editor and retry browser storage to keep these changes.';
		} catch (error) {
			invalid = true;
			message = (error as Error).message;
		}
	}
	function restore() {
		try {
			rows = parseCompanies(asset('CORPNAME.ORI'));
			dirty = true;
			fill();
			invalid = false;
			message =
				'Original companies restored in this editor. Save to keep this change.';
		} catch {
			invalid = true;
			message = 'Reopen wsr985.exe to load the original company list.';
		}
	}
	function close() {
		if (dirty) {
			discard = true;
			message = 'There are unsaved edits. Save them or choose Discard edits.';
		} else closePanel();
	}
</script>

<Dialog.Root
	open
	onOpenChange={(open) => {
		if (!open) close();
	}}
>
	<Dialog.Content
		class="wsr-dialog customizer flex max-h-[90dvh] flex-col sm:max-w-4xl"
		showCloseButton={false}
		onEscapeKeydown={(event) => {
			if (dirty) {
				event.preventDefault();
				close();
			}
		}}
		onInteractOutside={(event) => {
			if (dirty) {
				event.preventDefault();
				close();
			}
		}}
	>
		<Dialog.Header class="shrink-0"
			><Dialog.Title>Company Customizer</Dialog.Title><Dialog.Description
				>Change names, symbols and home countries for future new games. The
				current game keeps its existing companies.</Dialog.Description
			></Dialog.Header
		>
		<div class="grid min-h-0 gap-6 overflow-auto md:grid-cols-2">
			<Field.Group>
				<Field.Field
					><Field.Label for="company-search">Find a company</Field.Label><Input
						id="company-search"
						data-search
						type="search"
						bind:value={query}
						placeholder="Name, symbol or company number"
					/></Field.Field
				>
				<Field.Field
					><Field.Label for="company-list">Companies</Field.Label><select
						id="company-list"
						data-list
						size="10"
						bind:value={selected}
						onchange={fill}
						disabled={$game.busy}
						class="min-h-60 w-full border border-input bg-background p-2 text-sm"
						>{#each matches as row (row.id)}<option value={String(row.id)}
								>{row.symbol} · {row.name}</option
							>{/each}</select
					><Field.Description
						>{matches.length} of {rows.length} companies</Field.Description
					></Field.Field
				>
			</Field.Group>
			<form onsubmit={apply}>
				<Field.Group>
					<p class="text-sm text-muted-foreground">Company {selected}</p>
					<Field.Field data-invalid={invalid}
						><Field.Label for="company-name">Company name</Field.Label><Input
							id="company-name"
							name="name"
							bind:value={name}
							maxlength={25}
							required
							disabled={$game.busy}
							aria-invalid={invalid}
						/></Field.Field
					>
					<Field.Field data-invalid={invalid}
						><Field.Label for="company-symbol">Stock symbol</Field.Label><Input
							id="company-symbol"
							name="symbol"
							bind:value={symbol}
							maxlength={5}
							pattern={'[A-Za-z]{1,5}'}
							required
							disabled={$game.busy}
							aria-invalid={invalid}
						/></Field.Field
					>
					<Field.Field
						><Field.Label for="company-country">Home country</Field.Label
						><NativeSelect.Root
							id="company-country"
							name="country"
							bind:value={country}
							disabled={$game.busy}
							class="w-full"
							>{#each $game.countries as label, id}<NativeSelect.Option
									value={String(id)}>{label}</NativeSelect.Option
								>{/each}</NativeSelect.Root
						></Field.Field
					>
					<Button type="submit" disabled={$game.busy}>Apply edit</Button>
				</Field.Group>
			</form>
		</div>
		{#if message}<Alert.Root variant={invalid ? 'destructive' : 'default'}
				><Alert.Description role="status" data-status
					>{message}</Alert.Description
				></Alert.Root
			>{:else}<p data-status role="status"></p>{/if}
		<Dialog.Footer class="shrink-0 flex-wrap"
			><Button
				data-restore
				variant="outline"
				disabled={$game.busy}
				onclick={restore}>Restore original companies</Button
			><Button data-save disabled={!dirty || $game.busy} onclick={save}
				>Save for new games</Button
			><Button
				data-close
				variant="outline"
				disabled={$game.busy}
				onclick={close}>Close</Button
			>{#if discard}<Button
					data-discard
					variant="destructive"
					onclick={closePanel}>Discard edits</Button
				>{/if}</Dialog.Footer
		>
	</Dialog.Content>
</Dialog.Root>
